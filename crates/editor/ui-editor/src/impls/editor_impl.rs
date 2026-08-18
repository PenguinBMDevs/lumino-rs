//! Editor 核心方法
//!
//! 包含构造函数、内存统计、音频动作。
//! 远端光标/Ctrl 键状态在 `impls::editor_impl::remote`，
//! 提交相关（pending 拖动/复制、异步提交）在 `impls::editor_impl::commit`，
//! 撤销/重做（路径历史优先）在 `impls::editor_impl::history`。
//!
//! 注意：`update_playback_key_colors` 在 `impls::playback`，
//! `update_selection_box_animation` 在 `impls::selection_box_anim`。

mod commit;
mod history;
mod remote;

#[cfg(test)]
mod tests;

use crate::velocity::VelocityPanel;
use crate::{EditState, Editor, EditorMemory, SpatialIndexState, grid};
use iced_widget::canvas;
use lumino_ui_core::message::AudioAction;
use std::cell::Cell;

impl Editor {
    /// 创建新的编辑器实例
    pub fn new() -> Self {
        // 使用 UI 内存标签包裹编辑器初始化，便于内存监控归因
        lumino_diagnostics::memtrace::with_tag(lumino_diagnostics::memtrace::AllocTag::Ui, || {
            Self {
                editor_state: crate::editor_state::EditorState::new(),
                grid_cache: canvas::Cache::new(),
                keyboard_cache: canvas::Cache::new(),
                ruler_cache: canvas::Cache::new(),
                spatial: SpatialIndexState::default(),
                remote_cursors: std::collections::HashMap::new(),
                playback_position: 0.0,
                playback_key_colors: [0u8; 1024], // 256 keys × 4 bytes
                playback_key_colors_enabled: false,
                loop_range: Some(grid::LoopRange::new()),
                notes_changed: false,
                pending_drag_state: None,
                pending_copy_drag_state: None,
                velocity_panel: VelocityPanel::new(),
                selection_box_anim: Cell::new(None),
                cached_selection_bounds: Cell::new(None),
                context_menu: crate::context_menu::PianoRollContextMenuState::default(),
                selected_bounds: Cell::new(None),
                playback_scan_state: crate::impls::PlaybackScanState::default(),
                ctrl_pressed: false,
            }
        })
    }

    /// 收集编辑器各组件的内存占用快照
    pub fn memory_breakdown(&self) -> EditorMemory {
        let d = &self.editor_state.data;

        // 2026-08 单一权威源：`notes` / `track_notes` 缓存已删除，
        // 音符统计全部从 document（唯一权威）读取。
        // 2026-08-15 统计口径收敛：删除 `notes_bytes` / `track_notes_bytes`
        // 两个与 `document_events_bytes` 重复统计同一份数据的字段，
        // 字节统计只看 `document_events_bytes`（唯一真实持有）。

        // 全量音符统计（document 各轨之和）
        let track_notes_entries = d.document.as_ref().map(|doc| doc.notes.len()).unwrap_or(0);
        let mut track_notes_count = 0usize;
        if let Some(doc) = &d.document {
            for notes in &doc.notes {
                track_notes_count += notes.len();
            }
        }

        // document notes (NoteEvent=16B) 真实占用
        let doc_notes_cap: usize = d
            .document
            .as_ref()
            .map(|d| d.notes.iter().map(|v| v.capacity()).sum())
            .unwrap_or(0);
        let doc_events_bytes = d
            .document
            .as_ref()
            .map(|doc| {
                doc_notes_cap * std::mem::size_of::<lumino_midi_loader::NoteEvent>() // NoteEvent
                    + doc.tempo_changes.capacity() * 8 // (u32, f32)
            })
            .unwrap_or(0);

        tracing::info!(
            "[MEMORY_DEBUG] document={}, notes_cap={}, track_notes_entries={}, track_notes_count={}",
            d.document.is_some(),
            doc_notes_cap,
            track_notes_entries,
            track_notes_count,
        );

        EditorMemory {
            track_notes_count,
            track_notes_entries,
            document_events_bytes: doc_events_bytes,
        }
    }

    /// 获取并清空待处理的音频动作
    pub fn take_audio_actions(&mut self) -> Vec<AudioAction> {
        let interaction = &mut self.editor_state.interaction;
        if matches!(
            interaction.edit_state,
            EditState::DraggingSelection { .. } | EditState::DraggingSelectionCopy { .. }
        ) {
            // 批量拖动中：预览序列按 BPM 时序弹出（play_at 到达的音符逐个发声）
            interaction.drain_preview_sequence(std::time::Instant::now());
        } else {
            // 中断兜底：非批量拖动状态仍残留序列（切轨/切工具等未走 released 的
            // 中断路径）→ 直接丢弃，避免松手后继续发声
            interaction.clear_preview_sequence();
        }
        let actions = interaction.take_audio_actions();
        if !actions.is_empty() {
            tracing::debug!("Editor: 取出了 {} 个音频动作", actions.len());
        }
        actions
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
