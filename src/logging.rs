use tracing::Level;
use tracing_subscriber::{
    EnvFilter, filter::filter_fn, fmt, layer::SubscriberExt, util::SubscriberInitExt,
};

pub fn init() {
    // 控制我们的 crate 的日志级别
    // 当环境变量 'RUST_LOG' 未指定时，默认为 INFO+
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let level_filter = filter_fn(|metadata| {
        if metadata.target().starts_with("lumino") {
            // 让 env_filter 接管控制
            true
        } else {
            // 对于框架和依赖项，我们只接受 WARN 和 ERROR，不包括 INFO
            metadata.level() < &Level::INFO
        }
    });

    let layer = fmt::layer()
        // 我们也可以使用 `pretty()`，但它有点太繁琐了
        .compact();

    tracing_subscriber::registry()
        .with(level_filter)
        .with(env_filter)
        .with(layer)
        .init();
}
