use tracing_subscriber::{EnvFilter, filter::filter_fn, fmt, layer::SubscriberExt, util::SubscriberInitExt};
use tracing::Level;

pub fn init() {
    // Controls log levels for our crates.
    // Default to INFO+ when the environment variable `RUST_LOG` is not specified.
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let level_filter = filter_fn(|metadata| {
        if metadata.target().starts_with("lumino") {
            // Let env_filter take over control.
            true
        } else {
            // For the framework, dependencies, we accept only WARN, ERROR, except the INFO.
            metadata.level() < &Level::INFO
        }
    });

    let layer = fmt::layer()
        // We could use `pretty()` either, but it's a little bit too annoying.
        .compact();

    tracing_subscriber::registry()
        .with(level_filter)
        .with(env_filter)
        .with(layer)
        .init();
}
