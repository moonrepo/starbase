//! Shared helpers for the OTLP exporter integration tests. Each transport
//! (gRPC, HTTP) lives in its own test binary so that the process-global tracing
//! subscriber is installed exactly once per test.
#![cfg(feature = "otel")]
#![allow(dead_code)]

use opentelemetry_proto::tonic::collector::logs::v1::{
    ExportLogsServiceRequest, ExportLogsServiceResponse, logs_service_server::LogsService,
};
use opentelemetry_proto::tonic::collector::metrics::v1::{
    ExportMetricsServiceRequest, ExportMetricsServiceResponse,
    metrics_service_server::MetricsService,
};
use opentelemetry_proto::tonic::collector::trace::v1::{
    ExportTraceServiceRequest, ExportTraceServiceResponse, trace_service_server::TraceService,
};
use std::env;
use std::sync::Arc;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};

/// A gRPC OTLP collector that records every export request it receives, shared
/// by the transport tests that exercise the tonic path.
#[derive(Clone, Default)]
pub struct Collector {
    pub log_requests: Arc<Mutex<Vec<ExportLogsServiceRequest>>>,
    pub metric_requests: Arc<Mutex<Vec<ExportMetricsServiceRequest>>>,
    pub trace_requests: Arc<Mutex<Vec<ExportTraceServiceRequest>>>,
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

/// Sets an environment variable for the duration of a test, restoring the
/// previous value (or absence) on drop.
pub struct EnvVarGuard {
    key: &'static str,
    old_value: Option<String>,
}

impl EnvVarGuard {
    pub fn set(key: &'static str, value: String) -> Self {
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

/// Reads a single HTTP/1.1 request from `stream`, returning its request-target
/// path and body. Returns `None` on a clean connection close so the caller can
/// stop reading a keep-alive connection.
pub async fn read_http_request(stream: &mut TcpStream) -> Option<(String, Vec<u8>)> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];

    // Read until the end of the header block, then keep any bytes that already
    // belong to the body.
    let header_end = loop {
        if let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break index;
        }

        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
    };

    let headers = String::from_utf8_lossy(&buffer[..header_end]).into_owned();
    let mut lines = headers.split("\r\n");
    let path = lines.next()?.split_whitespace().nth(1)?.to_owned();
    let content_length = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);

    let mut body = buffer[header_end + 4..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }

    Some((path, body))
}

/// Writes a `200 OK` protobuf response back to the exporter.
pub async fn write_http_response(stream: &mut TcpStream, body: Vec<u8>) {
    let head = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/x-protobuf\r\ncontent-length: {}\r\n\r\n",
        body.len()
    );

    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(&body).await;
    let _ = stream.flush().await;
}

/// Shared assertions over the telemetry a collector received, regardless of the
/// OTLP transport (gRPC or HTTP) used to deliver it.
pub fn assert_exported_telemetry(
    trace_requests: &[ExportTraceServiceRequest],
    metric_requests: &[ExportMetricsServiceRequest],
    log_requests: &[ExportLogsServiceRequest],
) {
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
}
