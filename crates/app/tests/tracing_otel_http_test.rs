#![cfg(feature = "otel")]

mod common;

use common::{EnvVarGuard, assert_exported_telemetry, read_http_request, write_http_response};
use opentelemetry::{KeyValue, global};
use opentelemetry_proto::tonic::collector::logs::v1::{
    ExportLogsServiceRequest, ExportLogsServiceResponse,
};
use opentelemetry_proto::tonic::collector::metrics::v1::{
    ExportMetricsServiceRequest, ExportMetricsServiceResponse,
};
use opentelemetry_proto::tonic::collector::trace::v1::{
    ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use prost::Message as _;
use serial_test::serial;
use starbase::tracing::{
    OtelOptions, OtelProtocol, TracingOptions, info, info_span, setup_tracing,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, oneshot};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn exports_spans_over_otlp_http() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let log_requests = Arc::new(Mutex::new(Vec::new()));
    let metric_requests = Arc::new(Mutex::new(Vec::new()));
    let trace_requests = Arc::new(Mutex::new(Vec::new()));
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

    let server = {
        let log_requests = Arc::clone(&log_requests);
        let metric_requests = Arc::clone(&metric_requests);
        let trace_requests = Arc::clone(&trace_requests);

        tokio::spawn(async move {
            loop {
                let mut stream = tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accepted = listener.accept() => accepted.unwrap().0,
                };

                let log_requests = Arc::clone(&log_requests);
                let metric_requests = Arc::clone(&metric_requests);
                let trace_requests = Arc::clone(&trace_requests);

                // Each OTLP/HTTP export is a POST to /v1/{signal}; decode the
                // protobuf body and reply with the matching empty response.
                tokio::spawn(async move {
                    while let Some((path, body)) = read_http_request(&mut stream).await {
                        let response = if path.ends_with("/v1/traces") {
                            if let Ok(request) = ExportTraceServiceRequest::decode(body.as_slice())
                            {
                                trace_requests.lock().await.push(request);
                            }
                            ExportTraceServiceResponse {
                                partial_success: None,
                            }
                            .encode_to_vec()
                        } else if path.ends_with("/v1/metrics") {
                            if let Ok(request) =
                                ExportMetricsServiceRequest::decode(body.as_slice())
                            {
                                metric_requests.lock().await.push(request);
                            }
                            ExportMetricsServiceResponse {
                                partial_success: None,
                            }
                            .encode_to_vec()
                        } else if path.ends_with("/v1/logs") {
                            if let Ok(request) = ExportLogsServiceRequest::decode(body.as_slice()) {
                                log_requests.lock().await.push(request);
                            }
                            ExportLogsServiceResponse {
                                partial_success: None,
                            }
                            .encode_to_vec()
                        } else {
                            Vec::new()
                        };

                        write_http_response(&mut stream, response).await;
                    }
                });
            }
        })
    };

    // With a generic endpoint, the HTTP exporter appends the /v1/{signal} path.
    let _endpoint = EnvVarGuard::set("OTEL_EXPORTER_OTLP_ENDPOINT", format!("http://{addr}"));
    let _protocol = EnvVarGuard::set("OTEL_EXPORTER_OTLP_PROTOCOL", "http/protobuf".into());

    let guard = setup_tracing(TracingOptions {
        otel: OtelOptions {
            enabled: true,
            logs_enabled: true,
            protocol: OtelProtocol::Http,
            service_name: Some("starbase-test".into()),
        },
        ..TracingOptions::default()
    })
    .unwrap();

    let span = info_span!("otlp-test-span", phase = "execute");
    let _entered = span.enter();
    info!("hello from starbase");
    let meter = global::meter("starbase-test-meter");
    let counter = meter.u64_counter("starbase.test.counter").build();
    let histogram = meter
        .u64_histogram("starbase.test.duration")
        .with_unit("ms")
        .build();

    counter.add(1, &[KeyValue::new("kind", "smoke")]);
    histogram.record(42, &[KeyValue::new("kind", "smoke")]);
    drop(_entered);
    drop(span);
    drop(guard);

    let wait_result = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if !trace_requests.lock().await.is_empty()
                && !metric_requests.lock().await.is_empty()
                && !log_requests.lock().await.is_empty()
            {
                break;
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;

    if wait_result.is_err() {
        let trace_count = trace_requests.lock().await.len();
        let metric_count = metric_requests.lock().await.len();
        let log_count = log_requests.lock().await.len();

        panic!(
            "timed out waiting for starbase OTLP/HTTP export (traces={trace_count}, metrics={metric_count}, logs={log_count})"
        );
    }

    let trace_requests = trace_requests.lock().await.clone();
    let metric_requests = metric_requests.lock().await.clone();
    let log_requests = log_requests.lock().await.clone();

    assert_exported_telemetry(&trace_requests, &metric_requests, &log_requests);

    let _ = shutdown_tx.send(());
    server.await.unwrap();
}
