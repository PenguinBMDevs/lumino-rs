//! 日志系统初始化
//!
//! 支持控制台输出（stderr）和异步非阻塞文件日志写盘。
//!
//! # 文件日志
//!
//! - 日志文件存储在 `{config_dir}/logs/lumino_{YYYYMMDD_HHMMSS}.log`
//! - 使用后台线程 + mpsc channel 实现异步非阻塞写入
//! - 支持自动轮转，默认保留最近 10 份日志文件
//! - 安装 panic hook 捕获 Rust 崩溃信息并写入日志

use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::{fs, thread};

use tracing::Level;
use tracing_subscriber::{
    EnvFilter, filter::filter_fn, fmt, layer::SubscriberExt, util::SubscriberInitExt,
};

/// 全局日志文件发送器（用于 ChannelWriter 异步写文件）
static LOG_SENDER: OnceLock<mpsc::Sender<String>> = OnceLock::new();

/// 日志文件目录（用于 panic hook 直接写入）
static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 日志保留份数（运行时动态更新，实际清理在下一次启动时生效）
static LOG_RETENTION: AtomicUsize = AtomicUsize::new(10);

/// 将日志消息写入文件后台线程的通道写适配器
///
/// 实现 `io::Write`，每次调用将消息通过 mpsc 发送到后台线程，
/// 由后台线程写入日志文件，实现异步非阻塞写入。
struct ChannelWriter;

impl Write for ChannelWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Some(sender) = LOG_SENDER.get() {
            let s = String::from_utf8_lossy(buf);
            let _ = sender.send(s.to_string());
        }
        // 即使未初始化也返回成功（此时消息仅输出到 stderr）
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// 初始化日志系统
///
/// 设置控制台日志输出（stderr），并添加文件日志层（暂不激活，等待 `start_file_logging` 启动）。
/// 安装 panic hook 以捕获崩溃信息。
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

    // 控制台输出层
    let stdout_layer = fmt::layer().compact();

    // 文件日志层（通过 ChannelWriter 异步写入，启动时暂不激活，等待 start_file_logging）
    let file_layer = fmt::layer()
        .with_writer(|| ChannelWriter)
        .with_ansi(false)
        .compact();

    tracing_subscriber::registry()
        .with(level_filter)
        .with(env_filter)
        .with(stdout_layer)
        .with(file_layer)
        .init();

    // 安装 panic hook
    install_panic_hook();
}

/// 启动文件日志
///
/// 在 `{log_dir}/` 下创建日志文件，启动后台线程异步写入。
/// 清理超过保留份数的旧日志文件。
///
/// 必须在 `init()` 之后调用，因为需要 tracing subscriber 已初始化。
pub fn start_file_logging(log_dir: PathBuf, retention: usize) {
    LOG_RETENTION.store(retention, Ordering::Relaxed);

    // 确保日志目录存在
    if let Err(e) = fs::create_dir_all(&log_dir) {
        tracing::warn!("创建日志目录失败: {}", e);
        return;
    }

    // 清理旧日志文件
    cleanup_old_logs(&log_dir, retention);

    // 保存日志目录路径（供 panic hook 直接写入）
    let _ = LOG_DIR.set(log_dir.clone());

    // 创建 mpsc 通道
    let (sender, receiver) = mpsc::channel::<String>();
    let (_shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

    let _ = LOG_SENDER.set(sender);

    // 启动后台写入线程
    let log_dir_for_thread = log_dir.clone();
    thread::Builder::new()
        .name("lumino-log-writer".into())
        .spawn(move || {
            let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
            let log_path = log_dir_for_thread.join(format!("lumino_{}.log", timestamp));

            let mut file = match fs::File::create(&log_path) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("[lumino] 无法创建日志文件 {}: {}", log_path.display(), e);
                    return;
                }
            };

            // 写入日志文件头
            let _ = writeln!(file, "--- Lumino Log Start: {} ---", chrono::Local::now());

            // 接收日志消息并写入文件
            loop {
                // 使用 select 风格：优先处理日志消息，检查关闭信号
                if shutdown_rx.try_recv().is_ok() {
                    // 收到关闭信号，写入最后一条消息后退出
                    let _ = writeln!(file, "--- Lumino Log End: {} ---", chrono::Local::now());
                    let _ = file.flush();
                    break;
                }

                // 阻塞接收日志消息
                match receiver.recv() {
                    Ok(msg) => {
                        if let Err(e) = write!(file, "{}", msg) {
                            eprintln!("[lumino] 写入日志失败: {}", e);
                            break;
                        }
                        if let Err(e) = file.flush() {
                            eprintln!("[lumino] 刷新日志失败: {}", e);
                            break;
                        }
                    }
                    Err(_) => {
                        // 发送端已断开，退出
                        break;
                    }
                }
            }
        })
        .ok();

    tracing::info!("文件日志已启动，存储目录: {}", log_dir.display());
}

/// 更新日志保留份数
///
/// 实际清理在下次启动时执行。
#[allow(dead_code)]
pub fn update_retention(retention: usize) {
    LOG_RETENTION.store(retention, Ordering::Relaxed);
}

/// 获取当前日志保留份数
#[allow(dead_code)]
pub fn retention_count() -> usize {
    LOG_RETENTION.load(Ordering::Relaxed)
}

/// 清理旧日志文件，仅保留最新的 `retention` 份
fn cleanup_old_logs(log_dir: &PathBuf, retention: usize) {
    if retention == 0 {
        return;
    }

    let dir = match fs::read_dir(log_dir) {
        Ok(d) => d,
        Err(_) => return,
    };

    let mut entries: Vec<_> = dir
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name_str = name.to_string_lossy();
            name_str.starts_with("lumino_") && name_str.ends_with(".log")
        })
        .collect();

    if entries.len() <= retention {
        return;
    }

    // 按修改时间排序（最旧的在前）
    entries.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()));

    // 删除超出保留份数的旧文件
    for entry in entries.iter().take(entries.len() - retention) {
        if let Err(e) = fs::remove_file(entry.path()) {
            eprintln!(
                "[lumino] 删除旧日志文件失败 {}: {}",
                entry.path().display(),
                e
            );
        }
    }
}

/// 安装 panic hook，捕获 Rust 崩溃信息并写入日志
fn install_panic_hook() {
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let location = panic_info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "<unknown location>".to_string());

        let payload = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic payload".to_string()
        };

        // 通过 tracing 写入日志（会同时输出到 stderr 和文件）
        tracing::error!("========== PANIC ==========");
        tracing::error!("Panic at: {}", location);
        tracing::error!("Message: {}", payload);
        tracing::error!("===========================");

        // 同时也尝试直接写入日志文件（确保即使 tracing 不可用也能记录）
        if let Some(log_dir) = LOG_DIR.get() {
            let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
            let crash_log_path = log_dir.join(format!("crash_{}.log", timestamp));
            if let Ok(mut f) = fs::File::create(&crash_log_path) {
                let _ = writeln!(f, "========== PANIC ==========");
                let _ = writeln!(f, "Timestamp: {}", chrono::Local::now());
                let _ = writeln!(f, "Panic at: {}", location);
                let _ = writeln!(f, "Message: {}", payload);
                let _ = writeln!(f, "===========================");
            }
        }

        // 调用原始 hook（stderr 输出 + backtrace）
        prev_hook(panic_info);
    }));
}
