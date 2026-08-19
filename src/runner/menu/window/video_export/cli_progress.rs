//! 命令行进度条（无外部依赖，纯 stdout 回车覆盖实现）
//!
//! 视频导出后台线程在终端输出一个单行进度动画，
//! 替代大量 `tracing::info!` 日志刷屏。

use std::io::{self, Write};

/// 简单的 ASCII/Unicode 命令行进度条。
pub struct CliProgressBar {
    /// 进度条宽度（字符数）
    width: usize,
    /// 当前阶段名称
    label: String,
    /// 上一行长度，用于清行
    last_line_len: usize,
}

impl CliProgressBar {
    /// 创建一个新进度条。
    ///
    /// `width` 为进度条内部长度（不含边框和文本）。
    /// `label` 为阶段名称，例如 `"MIDI解析"`、`"视频渲染"`。
    pub fn new(width: usize, label: impl Into<String>) -> Self {
        Self {
            width,
            label: label.into(),
            last_line_len: 0,
        }
    }

    /// 更新进度。
    ///
    /// `progress` 范围 0.0 ~ 1.0；`message` 显示在进度条右侧。
    pub fn update(&mut self, progress: f64, message: &str) {
        let progress = progress.clamp(0.0, 1.0);
        let filled = (progress * self.width as f64).round() as usize;
        let empty = self.width.saturating_sub(filled);
        let bar = "█".repeat(filled) + &"░".repeat(empty);
        let line = format!(
            "[{}] {:>3.0}% | {} | {}",
            bar,
            progress * 100.0,
            self.label,
            message
        );

        // 回到行首，用空格覆盖上一行残留内容，再输出新内容
        let clear_len = self.last_line_len.max(line.len());
        print!("\r{: <1$}\r{2}", "", clear_len, line);
        let _ = io::stdout().flush();
        self.last_line_len = line.len();
    }

    /// 标记完成，输出换行并保留最终消息。
    pub fn finish(&mut self, message: &str) {
        let line = format!("{} {}", self.label, message);
        let clear_len = self.last_line_len.max(line.len());
        print!("\r{: <1$}\r{2}\n", "", clear_len, line);
        let _ = io::stdout().flush();
        self.last_line_len = 0;
    }
}

impl Drop for CliProgressBar {
    fn drop(&mut self) {
        // 如果未调用 finish 就被释放（例如错误退出），
        // 至少补一个换行，避免后续日志粘在进度条后面。
        if self.last_line_len > 0 {
            println!();
            let _ = io::stdout().flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 进度条完整生命周期不应 panic。
    #[test]
    fn test_progress_bar_lifecycle() {
        let mut bar = CliProgressBar::new(10, "测试");
        bar.update(0.0, "开始");
        bar.update(0.5, "进行中");
        bar.update(1.0, "即将完成");
        bar.finish("完成");
    }

    /// 进度值越界时应被钳制到 [0.0, 1.0]，不应 panic。
    #[test]
    fn test_progress_bar_clamps_out_of_range() {
        let mut bar = CliProgressBar::new(10, "测试");
        bar.update(-0.5, "负值");
        bar.update(1.5, "超值");
        bar.finish("完成");
    }

    /// 未调用 finish 就 drop 时应安全补换行。
    #[test]
    fn test_progress_bar_drop_without_finish() {
        let mut bar = CliProgressBar::new(10, "测试");
        bar.update(0.5, "进行中");
        drop(bar);
    }
}
