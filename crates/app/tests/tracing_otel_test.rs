#![cfg(feature = "otel")]

use opentelemetry::{KeyValue, global};
use opentelemetry_proto::tonic::collector::logs::v1::{
    ExportLogsServiceRequest, ExportLogsServiceResponse,
    logs_service_server::{LogsService, LogsServiceServer},
};
use opentelemetry_proto::tonic::collector::metrics::v1::{
    ExportMetricsServiceRequest, ExportMetricsServiceResponse,
    metrics_service_server::{MetricsService, MetricsServiceServer},
};
use opentelemetry_proto::tonic::collector::trace::v1::{
    ExportTraceServiceRequest, ExportTraceServiceResponse,
    trace_service_server::{TraceService, TraceServiceServer},
};
use serial_test::serial;
use starbase::tracing::{OtelOptions, TracingOptions, info, info_span, setup_tracing};
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, oneshot};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status, transport::Server};

#[derive(Clone, Default)]
struct Collector {
    log_requests: Arc<Mutex<Vec<ExportLogsServiceRequest>>>,
    metric_requests: Arc<Mutex<Vec<ExportMetricsServiceRequest>>>,
    trace_requests: Arc<Mutex<Vec<ExportTraceServiceRequest>>>,
}

#[tonic::async_trait]
impl LogsService for Collector {
    async fn export(
        &self,
        request: Request<ExportLogsServiceRequest>,
    ) -> Result<Response<ExportLogsServiceResponse>, Status> {
        self.log_requests.lock().await.push(request.into_inner());

        Ok(Response::new(ExportLogsServiceResponse {
            partial_success: None,
        }))
    }
}

#[tonic::async_trait]
impl MetricsService for Collector {
    async fn export(
        &self,
        request: Request<ExportMetricsServiceRequest>,
    ) -> Result<Response<ExportMetricsServiceResponse>, Status> {
        self.metric_requests.lock().await.push(request.into_inner());

        Ok(Response::new(ExportMetricsServiceResponse {
            partial_success: None,
        }))
    }
}

#[tonic::async_trait]
impl TraceService for Collector {
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        self.trace_requests.lock().await.push(request.into_inner());

        Ok(Response::new(ExportTraceServiceResponse {
            partial_success: None,
        }))
    }
}

struct EnvVarGuard {
    key: &'static str,
    old_value: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: String) -> Self {
        let old_value = env::var(key).ok();

        unsafe {
            env::set_var(key, value);
        }

        Self { key, old_value }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(value) = &self.old_value {
            unsafe {
                env::set_var(self.key, value);
            }
        } else {
            unsafe {
                env::remove_var(self.key);
            }
        }
    }
}

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
    let service_names = trace_requests
        .iter()
        .flat_map(|request| request.resource_spans.iter())
        .flat_map(|resource| resource.resource.as_ref())
        .flat_map(|resource| resource.attributes.iter())
        .filter(|attribute| attribute.key == "service.name")
        .filter_map(|attribute| attribute.value.as_ref())
        .filter_map(|value| value.value.as_ref())
        .filter_map(|value| match value {
            opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(value) => {
                Some(value.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let metric_service_names = metric_requests
        .iter()
        .flat_map(|request| request.resource_metrics.iter())
        .flat_map(|resource| resource.resource.as_ref())
        .flat_map(|resource| resource.attributes.iter())
        .filter(|attribute| attribute.key == "service.name")
        .filter_map(|attribute| attribute.value.as_ref())
        .filter_map(|value| value.value.as_ref())
        .filter_map(|value| match value {
            opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(value) => {
                Some(value.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let scope_names = trace_requests
        .iter()
        .flat_map(|request| request.resource_spans.iter())
        .flat_map(|resource| resource.scope_spans.iter())
        .map(|scope| scope.scope.as_ref().map(|scope| scope.name.as_str()))
        .collect::<Vec<_>>();
    let spans = trace_requests
        .iter()
        .flat_map(|request| request.resource_spans.iter())
        .flat_map(|resource| resource.scope_spans.iter())
        .flat_map(|scope| scope.spans.iter())
        .collect::<Vec<_>>();
    let metric_names = metric_requests
        .iter()
        .flat_map(|request| request.resource_metrics.iter())
        .flat_map(|resource| resource.scope_metrics.iter())
        .flat_map(|scope| scope.metrics.iter())
        .map(|metric| metric.name.as_str())
        .collect::<Vec<_>>();
    let log_bodies = log_requests
        .iter()
        .flat_map(|request| request.resource_logs.iter())
        .flat_map(|resource| resource.scope_logs.iter())
        .flat_map(|scope| scope.log_records.iter())
        .filter_map(|record| record.body.as_ref())
        .filter_map(|body| body.value.as_ref())
        .filter_map(|value| match value {
            opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(value) => {
                Some(value.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let log_service_names = log_requests
        .iter()
        .flat_map(|request| request.resource_logs.iter())
        .flat_map(|resource| resource.resource.as_ref())
        .flat_map(|resource| resource.attributes.iter())
        .filter(|attribute| attribute.key == "service.name")
        .filter_map(|attribute| attribute.value.as_ref())
        .filter_map(|value| value.value.as_ref())
        .filter_map(|value| match value {
            opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(value) => {
                Some(value.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(
        spans.iter().any(|span| span.name == "otlp-test-span"),
        "expected exported spans to include otlp-test-span"
    );
    assert!(
        service_names.contains(&"starbase-test"),
        "expected exported spans to include the configured service.name resource"
    );
    assert!(
        metric_service_names.contains(&"starbase-test"),
        "expected exported metrics to include the configured service.name resource"
    );
    assert!(
        log_service_names.contains(&"starbase-test"),
        "expected exported logs to include the configured service.name resource"
    );
    assert!(
        scope_names.contains(&Some("starbase")),
        "expected exported spans to use the starbase instrumentation scope"
    );
    assert!(
        metric_names.contains(&"starbase.test.counter")
            && metric_names.contains(&"starbase.test.duration"),
        "expected exported metrics to include the test counter and histogram"
    );
    assert!(
        log_bodies
            .iter()
            .any(|body| body.contains("hello from starbase")),
        "expected exported OTLP logs to include the tracing event body"
    );

    let _ = shutdown_tx.send(());
    server.await.unwrap();
}
