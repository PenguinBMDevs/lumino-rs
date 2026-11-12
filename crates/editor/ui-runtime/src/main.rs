//! 原生 egui 运行时探针入口。

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    if let Err(error) = lumino_ui_runtime::run() {
        tracing::error!("运行时退出: {error}");
    }
}
