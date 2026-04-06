use std::fmt;
use std::sync::OnceLock;

use opentelemetry::global;
use opentelemetry::trace::{TraceContextExt, TracerProvider as _};
use opentelemetry::KeyValue;
use opentelemetry_sdk::trace::TracerProvider;
use opentelemetry_sdk::Resource;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::fmt::{
    self as tracing_fmt, format::Writer, FmtContext, FormatEvent, FormatFields,
};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{EnvFilter, Registry};

use crate::error::{AppError, Result};

const DEFAULT_LOG_LEVEL: &str = "info";
static TELEMETRY_INIT_RESULT: OnceLock<std::result::Result<(), String>> = OnceLock::new();

/// Initializes global tracing with trace_id-rich logs.
#[tracing::instrument(skip(service_name))]
pub fn init_telemetry(service_name: &str) -> Result<()> {
    let init_result = TELEMETRY_INIT_RESULT
        .get_or_init(|| init_telemetry_once(service_name).map_err(|error| error.to_string()));

    match init_result {
        Ok(()) => Ok(()),
        Err(message) => Err(AppError::internal(message.clone())),
    }
}

fn init_telemetry_once(service_name: &str) -> Result<()> {
    let provider = build_tracer_provider(service_name);
    let tracer = provider.tracer(service_name.to_owned());
    global::set_tracer_provider(provider);

    let subscriber = Registry::default()
        .with(default_env_filter())
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .event_format(TraceIdFormatter),
        );

    tracing::subscriber::set_global_default(subscriber)
        .map_err(|error| AppError::internal(format!("global subscriber ayarlanamadi: {error}")))?;

    Ok(())
}

fn build_tracer_provider(service_name: &str) -> TracerProvider {
    let resource = Resource::new([KeyValue::new("service.name", service_name.to_owned())]);
    TracerProvider::builder().with_resource(resource).build()
}

fn default_env_filter() -> EnvFilter {
    match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => EnvFilter::new(DEFAULT_LOG_LEVEL),
    }
}

struct TraceIdFormatter;

impl<S, N> FormatEvent<S, N> for TraceIdFormatter
where
    S: tracing::Subscriber + for<'lookup> LookupSpan<'lookup>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> fmt::Result {
        let trace_id = tracing::Span::current()
            .context()
            .span()
            .span_context()
            .trace_id();

        write!(writer, "trace_id={trace_id} ")?;
        tracing_fmt::format().format_event(ctx, writer, event)
    }
}
