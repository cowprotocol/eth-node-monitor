use tracing_subscriber::prelude::*;

/// Initialize the tracing subscriber
/// Optionally configures the OpenTelemetry tracing layer if `otlp` is true
pub fn init(otlp: bool) {
    let fmt_layer = tracing_subscriber::fmt::layer();

    let base_layer = tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(fmt_layer);

    let final_layer: Box<dyn tracing::Subscriber + Send + Sync> = if otlp {
        // Initialize tracing
        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(opentelemetry_otlp::new_exporter().tonic())
            .install_batch(opentelemetry_sdk::runtime::Tokio)
            .expect("pipeline install failed");

        let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        Box::new(base_layer.with(telemetry_layer))
    } else {
        Box::new(base_layer)
    };

    // Initailize the configured tracing subscriber
    final_layer.init();
}
