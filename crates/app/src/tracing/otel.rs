use crate::tracing::{TracingError, TracingResult};
use opentelemetry::global;
use opentelemetry::metrics::MeterProvider as _;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, MetricExporter, SpanExporter};
use opentelemetry_sdk::{
    Resource,
    logs::SdkLoggerProvider,
    metrics::{PeriodicReader, SdkMeterProvider},
    trace::SdkTracerProvider,
};
use std::env;
use tracing::Subscriber;
use tracing_subscriber::{Layer, layer::SubscriberExt, registry::LookupSpan};

const OTEL_INSTRUMENTATION_SCOPE: &str = env!("CARGO_PKG_NAME");

#[derive(Default)]
pub struct OtelOptions {
    /// Whether to export traces and metrics over OTLP.
    pub enabled: bool,
    /// Whether to export tracing events as OTLP logs.
    pub logs_enabled: bool,
    /// Service name recorded in the emitted telemetry resource.
    pub service_name: Option<String>,
}

// OTEL providers do shut down on drop, but only when the last cloned handle is
// dropped. The tracing subscriber, trace layer, and global meter provider can
// all retain handles past TracingGuard, so keep the original providers here and
// shut them down explicitly when the guard drops. This makes short-lived CLI
// runs flush traces, metrics, and logs before exit.
pub(super) struct OtelGuard {
    logger_provider: Option<SdkLoggerProvider>,
    meter_provider: Option<SdkMeterProvider>,
    tracer_provider: Option<SdkTracerProvider>,
}

fn get_otel_service_name(options: &OtelOptions) -> String {
    options
        .service_name
        .clone()
        .or_else(|| {
            env::current_exe().ok().and_then(|path| {
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .map(ToOwned::to_owned)
            })
        })
        .unwrap_or_else(|| "starbase-app".into())
}

fn get_otel_resource(options: &OtelOptions) -> Resource {
    Resource::builder()
        .with_service_name(get_otel_service_name(options))
        .build()
}

fn report_otel_shutdown_error(signal: &str, error: impl std::fmt::Display) {
    eprintln!("Failed to shut down OTLP {signal} exporter: {error}");
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.logger_provider.take()
            && let Err(error) = provider.shutdown()
        {
            report_otel_shutdown_error("logs", error);
        }

        if let Some(provider) = self.meter_provider.take()
            && let Err(error) = provider.shutdown()
        {
            report_otel_shutdown_error("metrics", error);
        }

        if let Some(provider) = self.tracer_provider.take()
            && let Err(error) = provider.shutdown()
        {
            report_otel_shutdown_error("traces", error);
        }
    }
}

fn setup_otel_tracing(
    options: &OtelOptions,
    resource: Resource,
) -> TracingResult<Option<SdkTracerProvider>> {
    if !options.enabled {
        return Ok(None);
    }

    let exporter = SpanExporter::builder()
        .with_tonic()
        .build()
        .map_err(|error| TracingError::otlp_exporter("traces", error))?;

    Ok(Some(
        SdkTracerProvider::builder()
            .with_resource(resource)
            .with_batch_exporter(exporter)
            .build(),
    ))
}

fn setup_otel_metrics(
    options: &OtelOptions,
    resource: Resource,
) -> TracingResult<Option<SdkMeterProvider>> {
    if !options.enabled {
        return Ok(None);
    }

    let exporter = MetricExporter::builder()
        .with_tonic()
        .build()
        .map_err(|error| TracingError::otlp_exporter("metrics", error))?;

    let reader = PeriodicReader::builder(exporter).build();

    Ok(Some(
        SdkMeterProvider::builder()
            .with_reader(reader)
            .with_resource(resource)
            .build(),
    ))
}

fn setup_otel_logs(
    options: &OtelOptions,
    resource: Resource,
) -> TracingResult<Option<SdkLoggerProvider>> {
    if !options.logs_enabled {
        return Ok(None);
    }

    // Logs are opt-in separately because exporting every tracing event can be
    // much noisier than exporting spans and product metrics.
    let exporter = LogExporter::builder()
        .with_tonic()
        .build()
        .map_err(|error| TracingError::otlp_exporter("logs", error))?;

    Ok(Some(
        SdkLoggerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource)
            .build(),
    ))
}

pub(super) fn extend_subscriber<S>(
    subscriber: S,
    options: &OtelOptions,
) -> TracingResult<(impl Subscriber + Send + Sync + 'static, OtelGuard)>
where
    S: Subscriber + for<'span> LookupSpan<'span> + Send + Sync + 'static,
{
    let resource = get_otel_resource(options);
    let logger_provider = setup_otel_logs(options, resource.clone())?;
    let meter_provider = setup_otel_metrics(options, resource.clone())?;
    let tracer_provider = setup_otel_tracing(options, resource)?;

    if let Some(provider) = &meter_provider {
        // Product code records metrics through OpenTelemetry's global meter
        // provider, while spans/logs are bridged through tracing layers.
        global::set_meter_provider(provider.clone());
        let _ = provider.meter(OTEL_INSTRUMENTATION_SCOPE);
    }

    // Optional layers keep disabled signals out of the subscriber without
    // changing the subscriber type shape.
    let log_layer = logger_provider
        .as_ref()
        .map(OpenTelemetryTracingBridge::new)
        .boxed();
    let trace_layer = tracer_provider
        .as_ref()
        .map(|provider| {
            tracing_opentelemetry::layer().with_tracer(provider.tracer(OTEL_INSTRUMENTATION_SCOPE))
        })
        .boxed();

    Ok((
        subscriber.with(log_layer).with(trace_layer),
        OtelGuard {
            logger_provider,
            meter_provider,
            tracer_provider,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_explicit_service_name() {
        assert_eq!(
            get_otel_service_name(&OtelOptions {
                enabled: true,
                logs_enabled: false,
                service_name: Some("starbase-test".into()),
            }),
            "starbase-test"
        );
    }

    #[test]
    fn disables_otel_when_not_enabled() {
        let resource = get_otel_resource(&OtelOptions::default());

        assert!(
            setup_otel_tracing(&OtelOptions::default(), resource.clone())
                .unwrap()
                .is_none()
        );
        assert!(
            setup_otel_metrics(&OtelOptions::default(), resource.clone())
                .unwrap()
                .is_none()
        );
        assert!(
            setup_otel_logs(&OtelOptions::default(), resource)
                .unwrap()
                .is_none()
        );
    }
}
