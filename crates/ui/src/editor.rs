pub mod grid;
pub mod history;
pub mod note;
pub mod onion_bg_lod0;
pub mod onion_bg_pool;
pub mod onion_skin;
pub mod scrollbar_widget;
pub mod spatial_index;
pub mod velocity;

pub mod editor_state;

// 子模块
mod auto_scroll;
mod clipboard;
mod coords;
mod drag;
mod interaction;
mod note_ops;
mod onion_skin_editor;
mod onion_skin_ops;
mod rendering;
mod scroll;
mod settings;
mod track;

#[cfg(test)]
mod tests;

use crate::{message::AudioAction, toolbar::Tool};
use iced_core::Point;
use iced_widget::canvas;
use std::cell::{Cell, RefCell};

use note::Note;
pub use onion_bg_lod0::*;
pub use onion_bg_pool::*;
pub use onion_skin::OnionSkinConfig;
// 统一从 editor_state 导入（重构迁移）
pub use editor_state::{EditState, HitType, ViewState};

/// 缓存失效标志位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheInvalidation(u8);

impl CacheInvalidation {
    pub const NONE: Self = Self(0);
    pub const GRID: Self = Self(1 << 0);
    pub const KEYBOARD: Self = Self(1 << 1);
    pub const RULER: Self = Self(1 << 2);
    pub const ALL: Self = Self(0b111);
}

/// 钢琴卷帘编辑器
pub struct Editor {
    pub grid_cache: canvas::Cache<crate::Renderer>,
    /// 键盘缓存（只随垂直滚动变化）
    pub keyboard_cache: canvas::Cache<crate::Renderer>,
    /// 标尺缓存（只随水平滚动变化）
    pub ruler_cache: canvas::Cache<crate::Renderer>,

    /// 洋葱皮配置
    onion_skin_config: OnionSkinConfig,

    /// 协作远端用户光标信息（用户ID -> (位置, 颜色, 用户名)）
    pub remote_cursors: std::collections::HashMap<String, (Point, String, String)>,

    /// 演奏指示线位置（以 tick 为单位）
    pub playback_position: f32,

    /// 音符数据是否已变化（需要更新播放管理器）
    notes_changed: bool,

    /// 音符空间索引（惰性更新）
    pub note_index: RefCell<Option<spatial_index::NoteSpatialIndex>>,
    pub note_index_dirty: Cell<bool>,
    pub query_cache: RefCell<Vec<usize>>,

    /// 其他音轨的音符空间索引（用于洋葱皮等，懒加载）
    pub track_note_indices:
        RefCell<std::collections::HashMap<usize, spatial_index::NoteSpatialIndex>>,

    /// 统一状态管理
    pub editor_state: editor_state::EditorState,

    /// 力度编辑面板
    pub velocity_panel: velocity::VelocityPanel,

    /// 缓存的可见音轨索引（洋葱皮用），避免每帧重复计算
    pub cached_onion_track_indices: Vec<usize>,
    /// 缓存的可见音轨哈希值（不含颜色）
    pub cached_onion_track_hash: u64,
    /// 缓存的音轨颜色哈希值
    pub cached_onion_config_hash: u64,
    /// 缓存是否有效
    pub onion_cache_valid: bool,
}

/// 编辑器各组件的内存占用快照（字节）
#[derive(Debug, Clone, Default)]
pub struct EditorMemory {
    /// editor.notes 的估算内存（len × sizeof(Note) + 树形结构开销）
    pub notes_bytes: usize,
    /// track_notes HashMap 中所有 im::Vector 的音符总量
    pub track_notes_count: usize,
    pub track_notes_bytes: usize,
    /// track_notes 的条目数
    pub track_notes_entries: usize,
    /// document Arc 指向事件的 Vec 内存
    pub document_events_bytes: usize,
    /// 空间索引追踪条目数
    pub track_note_indices_entries: usize,
}

impl Editor {
    /// 收集编辑器各组件的内存占用快照
    pub fn memory_breakdown(&self) -> EditorMemory {
        let d = &self.editor_state.data;
        let note_size = std::mem::size_of::<Note>();

        // editor.notes
        let notes_len = d.notes.len();
        let notes_bytes = notes_len * note_size;

        // track_notes
        let track_notes_entries = d.track_notes.len();
        let mut track_notes_count = 0usize;
        let mut track_notes_bytes = 0usize;
        for notes in d.track_notes.values() {
            track_notes_count += notes.len();
            track_notes_bytes += notes.len() * note_size;
        }

        // document events (CompactEvent=12B, (u32,f32)=8B)
        let doc_is_some = d.document.is_some();
        let doc_event_cap = d
            .document
            .as_ref()
            .map(|d| d.events.capacity())
            .unwrap_or(0);
        let doc_events_bytes = d
            .document
            .as_ref()
            .map(|doc| {
                doc.events.capacity() * 12       // CompactEvent
                    + doc.tempo_changes.capacity() * 8 // (u32, f32)
            })
            .unwrap_or(0);

        // track_note_indices
        let track_note_indices_entries = self.track_note_indices.borrow().len();

        tracing::info!(
            "[MEMORY_DEBUG] document={}, events_cap={}, notes_len={}, track_notes_entries={}, track_notes_count={}",
            doc_is_some,
            doc_event_cap,
            notes_len,
            track_notes_entries,
            track_notes_count,
        );

        EditorMemory {
            notes_bytes,
            track_notes_count,
            track_notes_bytes,
            track_notes_entries,
            document_events_bytes: doc_events_bytes,
            track_note_indices_entries,
        }
    }

    pub fn new() -> Self {
        Self {
            editor_state: editor_state::EditorState::new(),
            grid_cache: canvas::Cache::new(),
            keyboard_cache: canvas::Cache::new(),
            ruler_cache: canvas::Cache::new(),
            onion_skin_config: OnionSkinConfig::new(),
            remote_cursors: std::collections::HashMap::new(),
            playback_position: 0.0,
            notes_changed: false,
            note_index: RefCell::new(None),
            note_index_dirty: Cell::new(true),
            query_cache: RefCell::new(Vec::new()),
            track_note_indices: RefCell::new(std::collections::HashMap::new()),
            velocity_panel: velocity::VelocityPanel::new(),
            cached_onion_track_indices: Vec::new(),
            cached_onion_track_hash: 0,
            cached_onion_config_hash: 0,
            onion_cache_valid: false,
        }
    }

    /// 设置当前工具（委托到 editor_state）
    pub fn set_tool(&mut self, tool: Tool) {
        self.editor_state.set_tool(tool);
    }

    /// 获取当前工具
    pub fn current_tool(&self) -> Tool {
        self.editor_state.tool
    }

    /// 更新远端鼠标位置
    pub fn update_remote_cursor(
        &mut self,
        user_id: std::sync::Arc<str>,
        x: f32,
        y: f32,
        color: std::sync::Arc<str>,
        username: std::sync::Arc<str>,
    ) {
        self.remote_cursors.insert(
            user_id.to_string(),
            (Point::new(x, y), color.to_string(), username.to_string()),
        );
    }

    /// 移除远端鼠标
    pub fn remove_remote_cursor(&mut self, user_id: &str) {
        self.remote_cursors.remove(user_id);
        self.grid_cache.clear();
    }

    /// 更新鼠标位置（由外部调用）
    pub fn update_cursor_position(&mut self, position: Option<Point>) {
        self.editor_state.update_cursor_position(position);
    }

    /// 更新 Canvas 偏移量（用于坐标转换）
    pub fn set_canvas_offset(&mut self, offset: Point) {
        self.editor_state.set_canvas_offset(offset);
    }

    /// 更新 Canvas 尺寸
    pub fn set_canvas_size(&mut self, size: Point) {
        self.editor_state.set_canvas_size(size);
    }

    /// 获取并清空待处理的音频动作
    pub fn take_audio_actions(&mut self) -> Vec<AudioAction> {
        let actions = self.editor_state.interaction.take_audio_actions();
        if !actions.is_empty() {
            tracing::debug!("Editor: 取出了 {} 个音频动作", actions.len());
        }
        actions
    }

    /// 设置总 ticks
    pub fn set_total_ticks(&mut self, total_ticks: u32) {
        self.editor_state.view.total_ticks = total_ticks;
        let max_scroll_x = total_ticks as f32 * self.editor_state.view.zoom_x;
        self.editor_state.max_scroll.x = max_scroll_x;
    }

    /// 设置 PPQ
    pub fn set_ppq(&mut self, ppq: u16) {
        self.editor_state.view.ppq = ppq;
    }

    /// 检查音符数据是否已变化
    pub fn notes_changed(&self) -> bool {
        self.notes_changed
    }

    /// 清除音符变化标志
    pub fn clear_notes_changed(&mut self) {
        self.notes_changed = false;
    }

    /// 统一缓存失效（替代散落的 grid_cache.clear() 等调用）
    #[inline]
    pub fn invalidate_caches(&mut self, which: CacheInvalidation) {
        if which.0 & CacheInvalidation::GRID.0 != 0 {
            self.grid_cache.clear();
        }
        if which.0 & CacheInvalidation::KEYBOARD.0 != 0 {
            self.keyboard_cache.clear();
        }
        if which.0 & CacheInvalidation::RULER.0 != 0 {
            self.ruler_cache.clear();
        }
    }

    /// 标记音符数据已变化
    pub fn mark_notes_changed(&mut self) {
        self.notes_changed = true;
        self.note_index_dirty.set(true);
        self.track_note_indices
            .borrow_mut()
            .remove(&self.editor_state.data.current_track);
    }

    /// 重置编辑器内部状态到默认值（释放私有字段内存）
    ///
    /// 供 `clear_editor()` 调用，重置本模块私有的字段：
    /// - `onion_skin_config`：洋葱皮配置
    /// - `notes_changed`：音符变更标志
    /// - `playback_position`：播放指示线位置
    pub fn reset_internal_state(&mut self) {
        use crate::editor::onion_skin::OnionSkinConfig;
        self.onion_skin_config = OnionSkinConfig::new();
        self.notes_changed = false;
        self.playback_position = 0.0;
        self.velocity_panel = velocity::VelocityPanel::new();
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    /// Push current state to history
    pub fn push_history(&mut self) {
        let d = &self.editor_state.data;
        let snapshot = history::EditorSnapshot::new(d.notes.clone(), d.current_track);
        tracing::debug!(
            "推送历史记录: {} 个音符，音轨 {}",
            snapshot.notes.len(),
            snapshot.current_track
        );
        self.editor_state.data.history.push(snapshot);
    }

    /// Undo the last action
    pub fn undo(&mut self) -> bool {
        let d = &self.editor_state.data;
        let current_state = history::EditorSnapshot::new(d.notes.clone(), d.current_track);
        tracing::info!(
            "尝试撤销: 当前音符数 = {}, 可撤销 = {}",
            d.notes.len(),
            self.can_undo()
        );

        if let Some(snapshot) = self.editor_state.data.history.undo(current_state) {
            self.editor_state.data.notes = snapshot.notes;
            self.editor_state.data.current_track = snapshot.current_track;
            self.grid_cache.clear();
            self.mark_notes_changed();
            tracing::info!(
                "撤销操作成功: {} 个音符",
                self.editor_state.data.notes.len()
            );
            true
        } else {
            tracing::info!("没有可撤销的操作");
            false
        }
    }

    /// Redo the last undone action
    pub fn redo(&mut self) -> bool {
        let d = &self.editor_state.data;
        let current_state = history::EditorSnapshot::new(d.notes.clone(), d.current_track);

        if let Some(snapshot) = self.editor_state.data.history.redo(current_state) {
            self.editor_state.data.notes = snapshot.notes;
            self.editor_state.data.current_track = snapshot.current_track;
            self.grid_cache.clear();
            self.mark_notes_changed();
            tracing::info!("重做操作成功");
            true
        } else {
            tracing::info!("没有可重做的操作");
            false
        }
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        self.editor_state.data.history.can_undo()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        self.editor_state.data.history.can_redo()
    }
}
