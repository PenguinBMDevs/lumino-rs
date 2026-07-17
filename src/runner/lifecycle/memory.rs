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

            let rss_mb =
                lumino_memory_monitor::MemoryMonitor::global().current_rss() / (1024 * 1024);

            let writer_total = mem.note_instances_writer_cap as u64 * mem.note_instance_size as u64;
            let ready_total = mem.note_instances_ready_cap as u64 * mem.note_instance_size as u64;
            let reading_total = mem.note_instances_reading_cap as u64 * mem.note_instance_size as u64;

            tracing::info!(
                "\n\
                ┌─ Memory Usage ──────────────────────────────────────────┐\n\
                │ 进程 RSS:              {:>8} MB                         │\n\
                ├─────────────────────────────────────────────────────────┤\n\
                │ MidiDocument.notes:    {:>8} MB  (Vec<NoteEvent>)      │\n\
                │ editor.notes:          {:>8} MB  (im::Vector<Note>)     │\n\
                │ track_notes({}条):  {:>8} MB  ({} 音符)              │\n\
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
                mem.editor.notes_bytes / (1024 * 1024),
                mem.editor.track_notes_entries,
                mem.editor.track_notes_bytes / (1024 * 1024),
                mem.editor.track_notes_count,
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
}
