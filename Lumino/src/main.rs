use tracing::{debug, info, warn, error};
use tracing_subscriber::EnvFilter;

// 打印运行环境版本
const LUMINO_VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    // 初始化日志系统（支持 RUST_LOG 环境变量控制日志级别）
    // 用法：RUST_LOG=debug cargo run
    //      RUST_LOG=info cargo run
    //      RUST_LOG=Lumino=debug cargo run
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    println!("Hello, world! Hello Lumino {}!", LUMINO_VERSION);

    // 使用不同级别的日志
    debug!("测试 debug");
    info!("测试 info");
    warn!("测试 warn");
    error!("测试 error");
}

