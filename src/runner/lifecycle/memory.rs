//! 内存日志功能模块

use std::time::{Duration, Instant};

use crate::runner::inner::RunnerInner;

impl RunnerInner {
    /// 处理内存日志记录
    pub(crate) fn handle_memory_logging(&mut self) {
        if !self.test_state.log_memory_usage {
            return;
        }

        let now = Instant::now();
        let should_log = self
            .test_state
            .last_memory_log
            .map(|last| now.duration_since(last) >= Duration::from_millis(2000))
            .unwrap_or(true);

        if should_log {
            self.test_state.last_memory_log = Some(now);
            let mem = self.window_state.window.ui().memory_breakdown();

            let rss_mb = lumino_diagnostics::memory_monitor::MemoryMonitor::global().current_rss()
                / (1024 * 1024);

            tracing::info!(
                "\n\
                ┌─ Memory Usage ──────────────────────────────────────────┐\n\
                │ 进程 RSS:              {:>8} MB                         │\n\
                ├─────────────────────────────────────────────────────────┤\n\
                │ MidiDocument.notes:    {:>8} MB  (16B/音符, 唯一持有)   │\n\
                │ 音符总数:               {:>8}  ({:>6} 条音轨)          │\n\
                └─────────────────────────────────────────────────────────┘",
                rss_mb,
                mem.editor.document_events_bytes / (1024 * 1024),
                mem.editor.track_notes_count,
                mem.editor.track_notes_entries,
            );
        }
    }
}
