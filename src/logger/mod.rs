use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use std::env;

/// Initializes the global tracing subscriber.
/// Format can be toggled via SILOKA_LOG_FORMAT="json" or default to human-readable.
pub fn init() {
    // 1. Filter level log dari env RUST_LOG
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // 2. Cek apakah format JSON diinginkan
    let is_json = env::var("SILOKA_LOG_FORMAT").map(|v| v == "json").unwrap_or(false);

    let subscriber = tracing_subscriber::registry().with(filter);

    if is_json {
        subscriber.with(
            fmt::layer()
                .json()
                .with_thread_ids(true)
                .with_target(true)
        ).init();
    } else {
        subscriber.with(
            fmt::layer()
                .with_thread_ids(true)
                .with_target(true)
        ).init();
    }
}