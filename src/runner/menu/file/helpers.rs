//! Runner 文件菜单共享辅助函数

use std::path::Path;

use crate::runner::RunnerInner;

/// 获取文件扩展名（小写）
pub(super) fn get_file_extension(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default()
}

/// 从文件路径获取文件名（不含扩展名），失败时返回 "untitled"
pub(super) fn get_file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "untitled".to_string())
}

impl RunnerInner {
    /// 导入后立即输出内存日志（此时尚未触发首帧渲染，能看到干净的后导入态）
    pub(super) fn log_memory_usage_after_import(&self) {
        if !self.test_state.log_memory_usage {
            return;
        }
        let mem = self.window_state.window.ui().memory_breakdown();
        let rss_mb = lumino_diagnostics::memory_monitor::MemoryMonitor::global().current_rss()
            / (1024 * 1024);
        let writer_total = mem.note_instances_writer_cap as u64 * mem.note_instance_size as u64;
        let ready_total = mem.note_instances_ready_cap as u64 * mem.note_instance_size as u64;
        let reading_total = mem.note_instances_reading_cap as u64 * mem.note_instance_size as u64;
        tracing::info!(
            "\n\
            ┌─ Memory Usage (post-import, pre-render) ──────────────┐\n\
            │ 进程 RSS:              {:>8} MB                         │\n\
            ├─────────────────────────────────────────────────────────┤\n\
            │ MidiDocument.notes:    {:>8} MB  (16B/音符, 唯一持有)   │\n\
            │ 音符总数:               {:>8}  ({:>6} 条音轨)          │\n\
            │ track_midi_events:     {:>8} MB  ({} 条)               │\n\
            ├─────────────────────────────────────────────────────────┤\n\
            │ note_instances(三缓冲):                                │\n\
            │   writer 缓冲:         {:>8} MB  (cap={}, len={})      │\n\
            │   ready 缓冲:          {:>8} MB  (cap={}, len={})      │\n\
            │   reading 缓冲:        {:>8} MB  (cap={}, len={})      │\n\
            │   三缓冲合计:          {:>8} MB                         │\n\
            └─────────────────────────────────────────────────────────┘",
            rss_mb,
            mem.editor.document_events_bytes / (1024 * 1024),
            mem.editor.track_notes_count,
            mem.editor.track_notes_entries,
            mem.track_midi_events_bytes / (1024 * 1024),
            mem.track_midi_events_entries,
            writer_total / (1024 * 1024),
            mem.note_instances_writer_cap,
            mem.note_instances_writer_len,
            ready_total / (1024 * 1024),
            mem.note_instances_ready_cap,
            mem.note_instances_ready_len,
            reading_total / (1024 * 1024),
            mem.note_instances_reading_cap,
            mem.note_instances_reading_len,
            (writer_total + ready_total + reading_total) / (1024 * 1024),
        );
    }
}
