#![cfg(feature = "otel")]

mod common;

use common::{Collector, EnvVarGuard, assert_exported_telemetry};
use opentelemetry::{KeyValue, global};
use opentelemetry_proto::tonic::collector::logs::v1::logs_service_server::LogsServiceServer;
use opentelemetry_proto::tonic::collector::metrics::v1::metrics_service_server::MetricsServiceServer;
use opentelemetry_proto::tonic::collector::trace::v1::trace_service_server::TraceServiceServer;
use serial_test::serial;
use starbase::tracing::{
    OtelOptions, OtelProtocol, TracingOptions, info, info_span, setup_tracing,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, oneshot};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn exports_spans_over_otlp() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let log_requests = Arc::new(Mutex::new(Vec::new()));
    let metric_requests = Arc::new(Mutex::new(Vec::new()));
    let trace_requests = Arc::new(Mutex::new(Vec::new()));
    let collector = Collector {
        log_requests: Arc::clone(&log_requests),
        metric_requests: Arc::clone(&metric_requests),
        trace_requests: Arc::clone(&trace_requests),
    };
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let server = tokio::spawn(async move {
        Server::builder()
            .add_service(
                LogsServiceServer::new(collector.clone())
                    .max_decoding_message_size(32 * 1024 * 1024),
            )
            .add_service(
                MetricsServiceServer::new(collector.clone())
                    .max_decoding_message_size(32 * 1024 * 1024),
            )
            .add_service(TraceServiceServer::new(collector))
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    let _endpoint = EnvVarGuard::set(
        "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
        format!("http://{addr}"),
    );
    let _protocol = EnvVarGuard::set("OTEL_EXPORTER_OTLP_TRACES_PROTOCOL", "grpc".into());
    let _metrics_endpoint = EnvVarGuard::set(
        "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
        format!("http://{addr}"),
    );
    let _metrics_protocol = EnvVarGuard::set("OTEL_EXPORTER_OTLP_METRICS_PROTOCOL", "grpc".into());
    let _logs_endpoint =
        EnvVarGuard::set("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT", format!("http://{addr}"));
    let _logs_protocol = EnvVarGuard::set("OTEL_EXPORTER_OTLP_LOGS_PROTOCOL", "grpc".into());

    let guard = setup_tracing(TracingOptions {
        otel: OtelOptions {
            enabled: true,
            logs_enabled: true,
            protocol: OtelProtocol::Grpc,
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
            "timed out waiting for starbase OTLP export (traces={trace_count}, metrics={metric_count}, logs={log_count})"
        );
    }

    let log_requests = log_requests.lock().await.clone();
    let trace_requests = trace_requests.lock().await.clone();
    let metric_requests = metric_requests.lock().await.clone();

    assert_exported_telemetry(&trace_requests, &metric_requests, &log_requests);

    let _ = shutdown_tx.send(());
    server.await.unwrap();
}
