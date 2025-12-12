//! EnderDebugger Rust 库
//!
//! 这个库实现了一个简单但功能齐全的日志系统，包含：
//! - EnderLogger：类似原先 C# 版本的单例日志器，支持写入主日志文件、兼容 LuminoLogViewer 的 JSON 文件，以及彩色控制台输出
//! - 日志级别与查看器解析/格式化工具

use chrono::Local;
use uuid::Uuid;
use once_cell::sync::OnceCell;
use regex::Regex;
use serde::Serialize;
use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{PathBuf};
use std::sync::{Arc, Mutex};
use ctrlc;

/// 日志级别
#[derive(Debug, PartialEq, PartialOrd, Clone, Copy)]
pub enum LogLevel {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
    Fatal = 4,
}

impl Default for LogLevel {
    fn default() -> Self {
        LogLevel::Info
    }
}

/// 日志查看器配置
#[derive(Debug, Clone)]
pub struct LogViewerConfig {
    pub enabled_levels: std::collections::HashSet<String>,
    pub search_term: Option<String>,
    pub follow_file: bool,
    pub max_lines: usize,
    pub show_timestamp: bool,
}

impl Default for LogViewerConfig {
    fn default() -> Self {
        let mut s = std::collections::HashSet::new();
        for l in ["DEBUG", "INFO", "WARN", "ERROR", "FATAL"] {
            s.insert(l.to_string());
        }
        Self {
            enabled_levels: s,
            search_term: None,
            follow_file: true,
            max_lines: 1000,
            show_timestamp: true,
        }
    }
}

#[derive(Serialize, Debug)]
struct ViewerEntry<'a> {
    // 为了兼容原始 C# JSON 命名（Timestamp, Level, Component, Message），
    // 我们在 Rust 字段使用 snake_case 并用 serde 的 rename 来匹配原有字段名
    #[serde(rename = "Timestamp")]
    pub timestamp: String,
    #[serde(rename = "Level")]
    pub level: &'a str,
    #[serde(rename = "Component")]
    pub component: &'a str,
    #[serde(rename = "Message")]
    pub message: &'a str,
}

/// EnderLogger: 单例日志器实现
pub struct EnderLogger {
    // 来源字段，类似原来 C# 的 _source
    source: String,
    // 是否启用了调试模式
    is_debug: bool,
    // 当前最小日志级别
    min_log_level: LogLevel,
    // 路径
    log_directory: PathBuf,
    main_log_path: PathBuf,
    #[allow(dead_code)]
    viewer_log_path: PathBuf,
    viewer_static_log_path: PathBuf,
    // 写入器（缓冲）
    main_writer: Option<Arc<Mutex<BufWriter<File>>>>,
    viewer_writer: Option<Arc<Mutex<BufWriter<File>>>>,
    viewer_static_writer: Option<Arc<Mutex<BufWriter<File>>>>,
}

// 单例容器（OnceCell）
static INSTANCE: OnceCell<Arc<Mutex<EnderLogger>>> = OnceCell::new();

impl EnderLogger {

    /// 初始化或获取单例
    /// - 如果传递 `source`，会用作日志来源字段
    /// - 如果多次调用只会在第一次初始化时创建文件与写入器
    pub fn init(source: Option<&str>) -> Arc<Mutex<Self>> {
        // 这里使用 OnceCell 创建单例
        let s = source.unwrap_or("EnderLogger").to_string();
        INSTANCE.get_or_init(|| {
            let logger = EnderLogger::new(&s);
            Arc::new(Mutex::new(logger))
        })
        .clone()
    }

    fn new(source: &str) -> Self {
        // 尝试根据 Lumino.sln 查找工程根，若无则使用当前目录
        let project_root = find_project_root().unwrap_or_else(|| std::env::current_dir().unwrap());
        let mut log_dir = project_root.clone();
        log_dir.push("EnderDebugger");
        log_dir.push("Logs");

        if !log_dir.exists() {
            let _ = create_dir_all(&log_dir);
        }

        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
        let main_log_path = log_dir.join(format!("EnderDebugger_{}.log", &timestamp));
        let viewer_log_path = log_dir.join(format!("LuminoLogViewer_{}.log", &timestamp));
        let viewer_static_log_path = log_dir.join("LuminoLogViewer.log");

        // 打开文件（覆盖），允许读写共享模式在 Windows 的 OpenOptions 默认行为足够
        // 打开主日志文件，如果创建失败则回退到包含 PID / GUID 的唯一文件名
        let main_writer = match OpenOptions::new().create(true).write(true).truncate(true).open(&main_log_path) {
            Ok(f) => Some(Arc::new(Mutex::new(BufWriter::new(f)))),
            Err(_) => {
                // 尝试带进程ID的回退名
                let fallback_name = format!("EnderDebugger_{}_{}_{}.log", &timestamp, std::process::id(), Local::now().timestamp_millis());
                let fallback_path = log_dir.join(fallback_name);
                match OpenOptions::new().create(true).write(true).truncate(true).open(&fallback_path) {
                    Ok(f) => Some(Arc::new(Mutex::new(BufWriter::new(f)))),
                    Err(_) => {
                        // 最后尝试 GUID
                        let guid_name = format!("EnderDebugger_{}.log", Uuid::new_v4());
                        let guid_path = log_dir.join(guid_name);
                        OpenOptions::new().create(true).write(true).truncate(true).open(&guid_path).ok().map(|f| Arc::new(Mutex::new(BufWriter::new(f))))
                    }
                }
            }
        };

        let viewer_writer = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&viewer_log_path)
            .ok()
            .map(|f| Arc::new(Mutex::new(BufWriter::new(f))));

        // 为兼容旧代码/外部工具，也维护一个固定文件名的 Viewer 日志：LuminoLogViewer.log
        // 这样主界面的 LogViewer 可以直接打开该文件而不需要解析索引或时间戳名称。
        let viewer_static_writer = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&viewer_static_log_path)
            .ok()
            .map(|f| Arc::new(Mutex::new(BufWriter::new(f))));

        // 写入或更新一个小索引文件 `LuminoLogViewer.current`，指示当前 timestamped Viewer 文件的名字
        let index_path = log_dir.join("LuminoLogViewer.current");
        if let Some(filename) = viewer_log_path.file_name().and_then(|s| s.to_str()) {
            let _ = std::fs::write(&index_path, filename);
        }

        let mut logger = EnderLogger {
            source: source.to_string(),
            is_debug: false,
            min_log_level: LogLevel::Info,
            log_directory: log_dir,
            main_log_path,
            viewer_log_path,
            viewer_static_log_path,
            main_writer,
            viewer_writer,
            viewer_static_writer,
        };

        // 解析命令行参数（如 --debug）
        logger.parse_command_line_args();

        // 注册 Ctrl-C 退出时刷新日志的处理器，确保在中断信号时尽量写入磁盘
        let _ = ctrlc::set_handler(move || {
            if let Some(inst) = INSTANCE.get() {
                if let Ok(mut g) = inst.lock() {
                    g.flush_all();
                }
            }
            // 重新抛出 Ctrl-C 以结束程序
            std::process::exit(0);
        });

        logger
    }

    /// 解析命令行参数，支持 --debug [level] 等
    fn parse_command_line_args(&mut self) {
        let args: Vec<String> = std::env::args().collect();
        for (i, arg) in args.iter().enumerate() {
            if arg == "--debug" {
                let next = args.get(i + 1).map(|s| s.as_str());
                if let Some(l) = next {
                    self.enable_debug(match l.to_lowercase().as_str() {
                        "error" => Some(LogLevel::Error),
                        "warn" | "warning" => Some(LogLevel::Warn),
                        "info" => Some(LogLevel::Info),
                        "debug" => Some(LogLevel::Debug),
                        _ => Some(LogLevel::Debug),
                    });
                } else {
                    self.enable_debug(Some(LogLevel::Debug));
                }
                return;
            }
        }
    }

    /// 强制 flush 并尝试 sync_all
    pub fn flush_all(&mut self) {
        if let Some(w) = &self.main_writer {
            if let Ok(mut guard) = w.lock() {
                let _ = guard.flush();
                let _ = guard.get_ref().sync_all();
            }
        }
        if let Some(w) = &self.viewer_writer {
            if let Ok(mut guard) = w.lock() {
                let _ = guard.flush();
                let _ = guard.get_ref().sync_all();
            }
        }
        if let Some(w) = &self.viewer_static_writer {
            if let Ok(mut guard) = w.lock() {
                let _ = guard.flush();
                let _ = guard.get_ref().sync_all();
            }
        }
    }

    /// 设置/启用调试模式
    pub fn enable_debug(&mut self, level: Option<LogLevel>) {
        self.is_debug = true;
        self.min_log_level = level.unwrap_or(LogLevel::Debug);
        self.info("EnderLogger", &format!("调试模式启用，最小级别={:?}", self.min_log_level));
    }

    /// 关闭调试模式
    pub fn disable_debug(&mut self) {
        self.is_debug = false;
        self.min_log_level = LogLevel::Info;
    }

    /// 判断是否应该记录
    fn should_log(&self, level: LogLevel) -> bool {
        if !self.is_debug { return false; }
        level >= self.min_log_level
    }

    pub fn debug(&mut self, component: &str, content: &str) {
        self.log(LogLevel::Debug, component, content);
    }
    pub fn info(&mut self, component: &str, content: &str) {
        self.log(LogLevel::Info, component, content);
    }
    pub fn warn(&mut self, component: &str, content: &str) {
        self.log(LogLevel::Warn, component, content);
    }
    pub fn error(&mut self, component: &str, content: &str) {
        self.log(LogLevel::Error, component, content);
    }
    pub fn fatal(&mut self, component: &str, content: &str) {
        self.log(LogLevel::Fatal, component, content);
    }

    /// 记录异常/错误（转成 Error 级别）
    pub fn log_exception<E: std::fmt::Display>(&mut self, err: E, component: &str, content: Option<&str>) {
        let mut sb = format!("异常: {}", err);
        if let Some(c) = content { sb = format!("{}\n{}", c, sb); }
        self.log(LogLevel::Error, component, &sb);
    }

    /// 内部通用日志记录方法
    fn log(&mut self, level: LogLevel, component: &str, content: &str) {
        if !self.should_log(level) {
            return;
        }

        // console 输出（带颜色）
        let now = Local::now();
        let timestamp = format!("{}.{:03}", now.format("%H:%M:%S"), now.timestamp_subsec_millis());
        let level_str = get_level_text(level);
        let level_color = get_level_color(level);
        let reset = "\x1b[0m";
        let formatted = format!("{}[{}] [{}] [{}] [{}] {}{}", level_color, timestamp, level_str, self.source, component, content, reset);

        println!("{}", formatted);

        // 写入主日志（只写 message 内容）
        self.write_to_file(content);

        // 写入 viewer 日志 JSON
        self.write_to_viewer(&level_str, component, content);
    }

    fn sanitize_message(message: &str) -> String {
        // 去除 ANSI 颜色码与多行合并
        let ansi_re = Regex::new("\\x1b\\[[0-9;]*[mK]").unwrap();
        let s = ansi_re.replace_all(message, "");
        let trimmed = s.replace("\r\n", " ").replace("\n", " ").trim().to_string();
        // 尝试从格式化行提取 message（极简版）
        let re_full = Regex::new(r"^\s*\[\d{2}:\d{2}:\d{2}\.\d{3}\]\s*\[\w+\]\s*\[[^\]]+\]\s*\[[^\]]+\]\s*(.*)$").unwrap();
        if let Some(caps) = re_full.captures(&trimmed) {
            if let Some(g1) = caps.get(1) {
                return g1.as_str().to_string();
            }
        }
        trimmed
    }

    fn write_to_file(&mut self, message: &str) {
        if let Some(w) = &self.main_writer {
            let sanitized = EnderLogger::sanitize_message(message);
            let mut guard = w.lock().unwrap();
            let _ = writeln!(guard, "{}", sanitized);
            let _ = guard.flush();
            // 强制写入磁盘，减少崩溃或截断风险
            let _ = guard.get_ref().sync_all();
        }
    }

    fn write_to_viewer(&mut self, level: &str, component: &str, content: &str) {
        let entry = ViewerEntry {
            timestamp: Local::now().to_rfc3339(),
            level,
            component,
            message: content,
        };
        if let Ok(json) = serde_json::to_string(&entry) {
            if let Some(w) = &self.viewer_writer {
                let mut guard = w.lock().unwrap();
                let _ = writeln!(guard, "{}", json);
                let _ = guard.flush();
                let _ = guard.get_ref().sync_all();
            }
            if let Some(w2) = &self.viewer_static_writer {
                let mut guard = w2.lock().unwrap();
                let _ = writeln!(guard, "{}", json);
                let _ = guard.flush();
                let _ = guard.get_ref().sync_all();
            }
        }
    }

    /// 返回日志目录（供外部查看）
    pub fn get_log_directory(&self) -> PathBuf {
        self.log_directory.clone()
    }

    /// 返回当前主日志文件路径
    pub fn get_current_log_file_path(&self) -> PathBuf {
        self.main_log_path.clone()
    }

    /// 返回 viewer 静态日志文件路径（LuminoLogViewer.log）
    pub fn get_viewer_static_log_path(&self) -> PathBuf {
        self.viewer_static_log_path.clone()
    }

    /// 读取并返回 viewer 文件的最后 N 行（按 config）
    pub fn read_existing_logs(&self, config: Option<&LogViewerConfig>) -> Vec<String> {
        let cfg = config.cloned().unwrap_or_default();
        let mut logs = Vec::new();

        // 优先使用最新的 LuminoLogViewer_*.log，然后回退到 LuminoLogViewer.log
        let mut glob_pattern = self.log_directory.clone();
        glob_pattern.push("LuminoLogViewer_*.log");

        // 简单实现：扫描目录
        let mut files: Vec<_> = std::fs::read_dir(&self.log_directory)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|d| d.path())
            .filter(|p| {
                if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                    return fname.starts_with("LuminoLogViewer_") && fname.ends_with(".log");
                }
                false
            })
            .collect();

        files.sort_by(|a, b| {
            let ta = a.metadata().and_then(|m| m.modified()).ok();
            let tb = b.metadata().and_then(|m| m.modified()).ok();
            ta.cmp(&tb)
        });

        let mut viewer_path: Option<PathBuf> = files.pop();
        if viewer_path.is_none() {
            let fallback = self.log_directory.join("LuminoLogViewer.log");
            if fallback.exists() {
                viewer_path = Some(fallback);
            }
        }

        if let Some(vp) = viewer_path {
            if let Ok(file) = File::open(&vp) {
                let reader = BufReader::new(file);
                let mut lines = Vec::new();
                for l in reader.lines() {
                    if let Ok(s) = l {
                        lines.push(s);
                        if lines.len() > cfg.max_lines {
                            lines.remove(0);
                        }
                    }
                }

                for ln in lines {
                    if let Some(processed) = process_log_line(&ln, &cfg) {
                        logs.push(processed);
                    }
                }
            }
        }

        logs
    }
}

impl Drop for EnderLogger {
    fn drop(&mut self) {
        // 在销毁时确保所有缓冲区写入磁盘
        if let Some(w) = &self.main_writer {
            if let Ok(mut guard) = w.lock() {
                let _ = guard.flush();
                if let Ok(inner) = guard.get_ref().try_clone() { let _ = inner.sync_all(); }
            }
        }
        if let Some(w) = &self.viewer_writer {
            if let Ok(mut guard) = w.lock() {
                let _ = guard.flush();
                if let Ok(inner) = guard.get_ref().try_clone() { let _ = inner.sync_all(); }
            }
        }
        if let Some(w) = &self.viewer_static_writer {
            if let Ok(mut guard) = w.lock() {
                let _ = guard.flush();
                if let Ok(inner) = guard.get_ref().try_clone() { let _ = inner.sync_all(); }
            }
        }
    }
}

// --- helper functions ---

fn get_level_text(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Debug => "DEBUG",
        LogLevel::Info => "INFO ",
        LogLevel::Warn => "WARN ",
        LogLevel::Error => "ERROR",
        LogLevel::Fatal => "FATAL",
    }
}

fn get_level_color(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Debug => "\x1b[36m",
        LogLevel::Info => "\x1b[32m",
        LogLevel::Warn => "\x1b[33m",
        LogLevel::Error => "\x1b[31m",
        LogLevel::Fatal => "\x1b[35m",
    }
}

fn find_project_root() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let target = dir.join("Lumino.sln");
        if target.exists() { return Some(dir); }
        if !dir.pop() { break; }
    }
    None
}

// 读取和解析日志行
pub fn process_log_line(line: &str, config: &LogViewerConfig) -> Option<String> {
    if line.trim().is_empty() { return None; }

    // 如果是JSON
    if line.trim_start().starts_with('{') {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            let level = v.get("Level").and_then(|x| x.as_str()).unwrap_or("");
            let _comp = v.get("Component").and_then(|x| x.as_str()).unwrap_or("");
            let msg = v.get("Message").and_then(|x| x.as_str()).unwrap_or("");
            if should_display_log(level, msg, config) {
                return Some(format_json_log(&v, config));
            }
            return None;
        }
    }

    // 新格式 [HH:mm:ss.fff] [LEVEL] [SOURCE] [COMPONENT] Message
    let re_new = Regex::new(r"\[(\d{2}:\d{2}:\d{2}\.\d{3})\]\s*\[(\w+)\]\s*\[([^\]]+)\]\s*\[([^\]]+)\]\s*(.*)").unwrap();
    if let Some(caps) = re_new.captures(line) {
        let timestamp = caps.get(1).unwrap().as_str();
        let level = caps.get(2).unwrap().as_str();
        let source = caps.get(3).unwrap().as_str();
        let component = caps.get(4).unwrap().as_str();
        let message = caps.get(5).unwrap().as_str();
        if should_display_log(level, message, config) {
            return Some(format_log_new(timestamp, level, source, component, message, config));
        }
        return None;
    }

    // 旧格式 [EnderDebugger][DATETIME][SOURCE][COMPONENT]Message
    let re_old = Regex::new(r"\[EnderDebugger\]\[([^\]]+)\]\[([^\]]+)\]\[([^\]]+)\]\s*(.*)").unwrap();
    if let Some(caps) = re_old.captures(line) {
        let datetime_str = caps.get(1).unwrap().as_str();
        let source = caps.get(2).unwrap().as_str();
        let component = caps.get(3).unwrap().as_str();
        let message = caps.get(4).unwrap().as_str();
        // 从 datetime 中提取 time
        let re_time = Regex::new(r"(\d{2}:\d{2}:\d{2}\.\d{3})").unwrap();
        let ts = re_time.captures(datetime_str).and_then(|c| c.get(1)).map(|m| m.as_str()).unwrap_or("00:00:00.000");
        if should_display_log("INFO", message, config) {
            return Some(format_log_new(ts, "INFO", source, component, message, config));
        }
        return None;
    }

    // 其他未解析行：按搜索条件判断
    if config.search_term.is_none() || line.to_lowercase().contains(&config.search_term.clone().unwrap_or_default().to_lowercase()) {
        return Some(line.to_string());
    }
    None
}

fn should_display_log(level: &str, message: &str, config: &LogViewerConfig) -> bool {
    if !config.enabled_levels.contains(&level.to_uppercase()) { return false; }
    if let Some(term) = &config.search_term {
        return message.to_lowercase().contains(&term.to_lowercase()) || level.to_lowercase().contains(&term.to_lowercase());
    }
    true
}

fn format_json_log(v: &serde_json::Value, config: &LogViewerConfig) -> String {
    let ts = v.get("Timestamp").and_then(|t| t.as_str()).unwrap_or("");
    let timestamp = if config.show_timestamp { ts.to_string() } else { "".to_string() };
    let level = v.get("Level").and_then(|l| l.as_str()).unwrap_or("");
    let comp = v.get("Component").and_then(|c| c.as_str()).unwrap_or("");
    let msg = v.get("Message").and_then(|m| m.as_str()).unwrap_or("");
    let level_txt = get_level_text_from_str(level);
    let level_color = get_level_color_from_str(level);
    let reset = "\x1b[0m";
    if config.show_timestamp {
        format!("{}[{}] [{}] [{}] [LogViewer] {}{}", level_color, timestamp, level_txt, comp, msg, reset)
    } else {
        format!("{}[{}] [{}] [LogViewer] {}{}", level_color, level_txt, comp, msg, reset)
    }
}

fn format_log_new(timestamp: &str, level: &str, source: &str, component: &str, message: &str, config: &LogViewerConfig) -> String {
    let level_txt = get_level_text_from_str(level);
    let level_col = get_level_color_from_str(level);
    let source_col = "\x1b[36m"; // 青色
    let comp_col = "\x1b[35m"; // 紫色
    let reset = "\x1b[0m";
    if config.show_timestamp {
        format!("{}[{}] [{}] {}[{}] {}[{}] {}{}", level_col, timestamp, level_txt, source_col, source, comp_col, component, message, reset)
    } else {
        format!("{}[{}] {}[{}] {}[{}] {}{}", level_col, level_txt, source_col, source, comp_col, component, message, reset)
    }
}

fn get_level_text_from_str(l: &str) -> &'static str {
    match l.trim().to_uppercase().as_str() {
        "DEBUG" => "DEBUG",
        "INFO" => "INFO ",
        "WARN" | "WARNING" => "WARN ",
        "ERROR" => "ERROR",
        "FATAL" => "FATAL",
        _ => "UNKNOWN",
    }
}

fn get_level_color_from_str(l: &str) -> &'static str {
    match l.trim().to_uppercase().as_str() {
        "DEBUG" => "\x1b[38;5;14m",
        "INFO" => "\x1b[38;5;10m",
        "WARN" => "\x1b[38;5;11m",
        "ERROR" => "\x1b[38;5;9m",
        "FATAL" => "\x1b[38;5;13m",
        _ => "\x1b[38;5;7m",
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_message() {
        let raw = "\u{1b}[31m[12:34:56.789] [INFO] [Source] [Comp] Message\u{1b}[0m\nAnother line";
        let s = EnderLogger::sanitize_message(raw);
        assert!(s.contains("Message"));
        assert!(!s.contains("\u{1b}"));
    }

    #[test]
    fn test_process_json_line() {
        let cfg = LogViewerConfig::default();
        let json = r#"{"Timestamp":"2025-12-12T12:00:00Z","Level":"INFO","Component":"X","Message":"Hello"}"#;
        let out = process_log_line(json, &cfg);
        assert!(out.is_some());
        let s = out.unwrap();
        assert!(s.contains("Hello"));
    }

    #[test]
    fn test_process_new_format() {
        let cfg = LogViewerConfig::default();
        let line = "[12:00:00.123] [DEBUG] [Source] [Comp] Test message";
        let out = process_log_line(line, &cfg);
        assert!(out.is_some());
        let s = out.unwrap();
        assert!(s.contains("Test message"));
    }
}
