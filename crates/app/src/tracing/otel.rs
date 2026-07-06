use crate::tracing::{TracingError, TracingResult};
use opentelemetry::global;
use opentelemetry::metrics::MeterProvider as _;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::tonic_types::transport::ClientTlsConfig;
use opentelemetry_otlp::{LogExporter, MetricExporter, SpanExporter, WithTonicConfig};
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

/// Transport used to deliver OTLP traces, metrics, and logs to the collector.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OtelProtocol {
    /// Defer to the standard OpenTelemetry environment variables to choose the
    /// transport per signal: `OTEL_EXPORTER_OTLP_{TRACES,METRICS,LOGS}_PROTOCOL`,
    /// then `OTEL_EXPORTER_OTLP_PROTOCOL`, falling back to `http/protobuf` when
    /// neither is set. Recognized values are `grpc` and `http/protobuf`.
    #[default]
    Auto,
    /// Force OTLP over gRPC (the `grpc` protocol), served on the collector's gRPC
    /// port, ignoring the protocol environment variables.
    Grpc,
    /// Force OTLP over HTTP with binary protobuf payloads (the `http/protobuf`
    /// protocol), POSTed to the collector's HTTP `/v1/{signal}` endpoints,
    /// ignoring the protocol environment variables.
    Http,
}

/// OpenTelemetry configuration for exporting traces, metrics, and logs over OTLP.
#[derive(Debug, Default)]
pub struct OtelOptions {
    /// Whether to export traces and metrics over OTLP. The environment can still
    /// force this off (see [`OtelOptions::logs_enabled`]) but cannot turn it on.
    pub enabled: bool,
    /// Whether to export tracing events as OTLP logs. Enabling a signal is always
    /// an explicit choice here, but the standard environment variables can force
    /// a signal off: `OTEL_SDK_DISABLED=true` disables every signal, and
    /// `OTEL_{TRACES,METRICS,LOGS}_EXPORTER=none` disables an individual one.
    pub logs_enabled: bool,
    /// Transport used to reach the OTLP collector, or [`OtelProtocol::Auto`] to
    /// select it from the environment. The endpoint is always read from the
    /// standard `OTEL_EXPORTER_OTLP_*` environment variables.
    pub protocol: OtelProtocol,
    /// Service name recorded in the emitted telemetry resource. When `None`, the
    /// name is resolved from the standard environment (`OTEL_SERVICE_NAME`, then
    /// `service.name` in `OTEL_RESOURCE_ATTRIBUTES`), falling back to the spec
    /// `unknown_service:<exe>` value.
    pub service_name: Option<String>,
}

// The gRPC and HTTP transports are selected by different builder methods that
// return distinct types, so this can't collapse into a plain function. Each arm
// still terminates in `.build()`, which yields the same exporter type, letting
// call sites stay transport-agnostic. Resolving the protocol ourselves (rather
// than deferring to the crate's bare `.build()`) lets the gRPC path attach a
// rooted TLS config, which the crate's automatic `https` handling omits.
macro_rules! build_otlp_exporter {
    ($builder:expr, $protocol:expr, $protocol_env:expr, $endpoint_env:expr) => {
        match resolve_signal_protocol($protocol, $protocol_env) {
            OtelProtocol::Grpc => {
                let builder = $builder.with_tonic();

                // TLS only applies to `https` endpoints; tonic ignores the config
                // for plaintext ones but still loads the OS trust store eagerly,
                // which is wasteful and can fail where none exists — so gate it.
                if is_https_endpoint($endpoint_env) {
                    builder.with_tls_config(otel_tls_config()).build()
                } else {
                    builder.build()
                }
            }
            // `resolve_signal_protocol` only ever yields `Grpc` or `Http`.
            _ => $builder.with_http().build(),
        }
    };
}

/// Resolves the transport for a signal, expanding [`OtelProtocol::Auto`] using the
/// standard OTLP protocol environment variables.
fn resolve_signal_protocol(protocol: OtelProtocol, protocol_env: &str) -> OtelProtocol {
    match protocol {
        OtelProtocol::Auto => protocol_from_env(protocol_env),
        explicit => explicit,
    }
}

fn protocol_from_env(protocol_env: &str) -> OtelProtocol {
    // The per-signal variable wins over the generic one; anything that isn't
    // `grpc` (including unset) resolves to HTTP, matching the crate's feature
    // default for the transports we enable.
    let value = env::var(protocol_env)
        .or_else(|_| env::var("OTEL_EXPORTER_OTLP_PROTOCOL"))
        .unwrap_or_default();

    if value.trim().eq_ignore_ascii_case("grpc") {
        OtelProtocol::Grpc
    } else {
        OtelProtocol::Http
    }
}

fn is_https_endpoint(endpoint_env: &str) -> bool {
    // The per-signal endpoint wins over the generic one; the OTLP default
    // endpoint is plaintext, so an unset endpoint is treated as non-TLS.
    env::var(endpoint_env)
        .or_else(|_| env::var("OTEL_EXPORTER_OTLP_ENDPOINT"))
        .is_ok_and(|endpoint| endpoint.trim().to_ascii_lowercase().starts_with("https://"))
}

fn otel_tls_config() -> ClientTlsConfig {
    // Verify the collector against the operating system's certificate store.
    ClientTlsConfig::new().with_native_roots()
}

// OTEL providers do shut down on drop, but only when the last cloned handle is
// dropped. The tracing subscriber, trace layer, and global meter provider can
// all retain handles past TracingGuard, so keep the original providers here and
// shut them down explicitly when the guard drops. This makes short-lived CLI
// runs flush traces, metrics, and logs before exit.
pub struct OtelGuard {
    logger_provider: Option<SdkLoggerProvider>,
    meter_provider: Option<SdkMeterProvider>,
    tracer_provider: Option<SdkTracerProvider>,
}

/// Whether a signal should be exported, combining the caller's opt-in with the
/// standard OpenTelemetry environment variables. The environment can only turn a
/// signal off, never on: `OTEL_SDK_DISABLED=true` disables all signals, and the
/// per-signal `OTEL_{TRACES,METRICS,LOGS}_EXPORTER=none` disables just that one.
fn is_signal_enabled(enabled: bool, exporter_env: &str) -> bool {
    enabled && !is_sdk_disabled() && !is_exporter_none(exporter_env)
}

fn is_sdk_disabled() -> bool {
    env::var("OTEL_SDK_DISABLED").is_ok_and(|value| value.trim().eq_ignore_ascii_case("true"))
}

fn is_exporter_none(exporter_env: &str) -> bool {
    env::var(exporter_env).is_ok_and(|value| value.trim().eq_ignore_ascii_case("none"))
}

fn get_otel_resource(options: &OtelOptions) -> Resource {
    // `Resource::builder()` runs the standard detectors, so `OTEL_SERVICE_NAME`
    // and `OTEL_RESOURCE_ATTRIBUTES` flow through and the spec `unknown_service:<exe>`
    // fallback applies. Only override the service name when the caller set one
    // explicitly, so ambient environment configuration isn't clobbered.
    let builder = Resource::builder();

    match &options.service_name {
        Some(service_name) => builder.with_service_name(service_name.clone()).build(),
        None => builder.build(),
    }
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
    if !is_signal_enabled(options.enabled, "OTEL_TRACES_EXPORTER") {
        return Ok(None);
    }

    let exporter = build_otlp_exporter!(
        SpanExporter::builder(),
        options.protocol,
        "OTEL_EXPORTER_OTLP_TRACES_PROTOCOL",
        "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT"
    )
    .map_err(|error| TracingError::OtlpExporterFailed {
        signal: "traces".into(),
        error,
    })?;

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
    if !is_signal_enabled(options.enabled, "OTEL_METRICS_EXPORTER") {
        return Ok(None);
    }

    let exporter = build_otlp_exporter!(
        MetricExporter::builder(),
        options.protocol,
        "OTEL_EXPORTER_OTLP_METRICS_PROTOCOL",
        "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT"
    )
    .map_err(|error| TracingError::OtlpExporterFailed {
        signal: "metrics".into(),
        error,
    })?;

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
    if !is_signal_enabled(options.logs_enabled, "OTEL_LOGS_EXPORTER") {
        return Ok(None);
    }

    // Logs are opt-in separately because exporting every tracing event can be
    // much noisier than exporting spans and product metrics.
    let exporter = build_otlp_exporter!(
        LogExporter::builder(),
        options.protocol,
        "OTEL_EXPORTER_OTLP_LOGS_PROTOCOL",
        "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT"
    )
    .map_err(|error| TracingError::OtlpExporterFailed {
        signal: "logs".into(),
        error,
    })?;

    Ok(Some(
        SdkLoggerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource)
            .build(),
    ))
}

pub fn extend_subscriber<S>(
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
    use serial_test::serial;

    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let prev = env::var(key).ok();

            unsafe {
                env::set_var(key, value);
            }

            Self { key, prev }
        }

        fn unset(key: &'static str) -> Self {
            let prev = env::var(key).ok();

            unsafe {
                env::remove_var(key);
            }

            Self { key, prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(value) => unsafe { env::set_var(self.key, value) },
                None => unsafe { env::remove_var(self.key) },
            }
        }
    }

    #[test]
    fn env_never_enables_a_disabled_signal() {
        // No environment variable can turn a signal on.
        assert!(!is_signal_enabled(false, "OTEL_TRACES_EXPORTER"));
    }

    #[test]
    #[serial]
    fn exporter_none_forces_a_signal_off() {
        let _sdk = EnvGuard::set("OTEL_SDK_DISABLED", "false");
        let _traces = EnvGuard::set("OTEL_TRACES_EXPORTER", "none");
        let _metrics = EnvGuard::set("OTEL_METRICS_EXPORTER", "otlp");

        assert!(!is_signal_enabled(true, "OTEL_TRACES_EXPORTER"));
        assert!(is_signal_enabled(true, "OTEL_METRICS_EXPORTER"));
    }

    #[test]
    #[serial]
    fn sdk_disabled_forces_all_signals_off() {
        // Case-insensitive, and overrides an explicit `otlp` exporter selection.
        let _sdk = EnvGuard::set("OTEL_SDK_DISABLED", "TRUE");
        let _traces = EnvGuard::set("OTEL_TRACES_EXPORTER", "otlp");

        assert!(!is_signal_enabled(true, "OTEL_TRACES_EXPORTER"));
        assert!(!is_signal_enabled(true, "OTEL_METRICS_EXPORTER"));
        assert!(!is_signal_enabled(true, "OTEL_LOGS_EXPORTER"));
    }

    #[test]
    fn explicit_protocol_ignores_env() {
        assert_eq!(
            resolve_signal_protocol(OtelProtocol::Grpc, "OTEL_EXPORTER_OTLP_TRACES_PROTOCOL"),
            OtelProtocol::Grpc
        );
        assert_eq!(
            resolve_signal_protocol(OtelProtocol::Http, "OTEL_EXPORTER_OTLP_TRACES_PROTOCOL"),
            OtelProtocol::Http
        );
    }

    #[test]
    #[serial]
    fn auto_protocol_reads_env_with_per_signal_precedence() {
        let resolve =
            || resolve_signal_protocol(OtelProtocol::Auto, "OTEL_EXPORTER_OTLP_TRACES_PROTOCOL");

        // Per-signal wins over the generic variable.
        {
            let _generic = EnvGuard::set("OTEL_EXPORTER_OTLP_PROTOCOL", "http/protobuf");
            let _signal = EnvGuard::set("OTEL_EXPORTER_OTLP_TRACES_PROTOCOL", "grpc");
            assert_eq!(resolve(), OtelProtocol::Grpc);
        }

        // Falls back to the generic variable when the per-signal one is absent.
        {
            let _signal = EnvGuard::unset("OTEL_EXPORTER_OTLP_TRACES_PROTOCOL");
            let _generic = EnvGuard::set("OTEL_EXPORTER_OTLP_PROTOCOL", "grpc");
            assert_eq!(resolve(), OtelProtocol::Grpc);
        }

        // Defaults to HTTP when neither is set.
        {
            let _signal = EnvGuard::unset("OTEL_EXPORTER_OTLP_TRACES_PROTOCOL");
            let _generic = EnvGuard::unset("OTEL_EXPORTER_OTLP_PROTOCOL");
            assert_eq!(resolve(), OtelProtocol::Http);
        }
    }

    #[test]
    #[serial]
    fn detects_https_endpoints_with_per_signal_precedence() {
        // Per-signal endpoint wins over the generic one.
        {
            let _generic = EnvGuard::set("OTEL_EXPORTER_OTLP_ENDPOINT", "http://collector:4317");
            let _signal = EnvGuard::set(
                "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
                "https://collector:4317",
            );
            assert!(is_https_endpoint("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT"));
        }

        // Falls back to the generic endpoint.
        {
            let _signal = EnvGuard::unset("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT");
            let _generic = EnvGuard::set("OTEL_EXPORTER_OTLP_ENDPOINT", "https://collector:4317");
            assert!(is_https_endpoint("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT"));
        }

        // Plaintext and unset endpoints are not TLS.
        {
            let _signal = EnvGuard::set(
                "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
                "http://collector:4317",
            );
            let _generic = EnvGuard::unset("OTEL_EXPORTER_OTLP_ENDPOINT");
            assert!(!is_https_endpoint("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT"));
        }
        {
            let _signal = EnvGuard::unset("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT");
            let _generic = EnvGuard::unset("OTEL_EXPORTER_OTLP_ENDPOINT");
            assert!(!is_https_endpoint("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT"));
        }
    }

    #[tokio::test]
    #[serial]
    async fn builds_grpc_exporter_for_https_endpoint() {
        // Regression: an `https` gRPC endpoint used to fail to build with
        // "uses HTTPS but no TLS feature is enabled". It now builds with the
        // native-roots TLS config attached (the connection itself is lazy).
        let _endpoint = EnvGuard::set(
            "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
            "https://localhost:4317",
        );
        let _protocol = EnvGuard::set("OTEL_EXPORTER_OTLP_TRACES_PROTOCOL", "grpc");

        let options = OtelOptions {
            enabled: true,
            protocol: OtelProtocol::Auto,
            ..OtelOptions::default()
        };
        let resource = get_otel_resource(&options);

        let provider = setup_otel_tracing(&options, resource)
            .expect("https gRPC exporter should build with TLS enabled");

        assert!(provider.is_some());

        if let Some(provider) = provider {
            let _ = provider.shutdown();
        }
    }

    #[test]
    fn explicit_service_name_overrides_resource() {
        let resource = get_otel_resource(&OtelOptions {
            enabled: true,
            service_name: Some("starbase-test".into()),
            ..OtelOptions::default()
        });

        assert_eq!(
            resource.get(&opentelemetry::Key::from_static_str("service.name")),
            Some(opentelemetry::Value::from("starbase-test"))
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
