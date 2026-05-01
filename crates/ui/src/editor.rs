pub mod grid;
pub mod history;
pub mod note;
pub mod onion_skin;
pub mod scrollbar_widget;
pub mod spatial_index;
pub mod state;

// 子模块
mod auto_scroll;
mod clipboard;
mod coords;
mod drag;
mod interaction;
mod note_ops;
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
use lumino_core::midi::MidiDocument;
use lumino_core::storage::config::{AutoScrollConfig, AutoScrollMode};
use lumino_gfx::NoteInstance;
use std::cell::{Cell, RefCell};
use std::sync::Arc;

use note::Note;
pub use onion_skin::OnionSkinConfig;
pub use state::ViewState;

#[derive(Debug, Clone, Default, PartialEq)]
pub enum EditState {
    #[default]
    Idle,
    /// 框选状态
    Selecting {
        start_pos: Point,
        current_pos: Point,
    },
    Drawing {
        start_tick: f32,
        key: u16,
        current_tick: f32,
    },
    /// 预备拖动状态：点击音符后等待判断是点击还是拖动
    PendingDrag {
        note_index: usize,
        start_pos: Point,
        original_tick: f32,
        original_key: u16,
    },
    Dragging {
        note_index: usize,
        offset_tick: f32,
        offset_key: i32,
        last_played_key: u16, // 上一次播放的音高，用于避免重复播放
        original_tick: f32,
        original_key: u16,
    },
    ResizingStart {
        note_index: usize,
        original_tick: f32,
        original_length: f32,
    },
    ResizingEnd {
        note_index: usize,
    },
    /// 擦洗状态：在时间轴上拖动来快速定位播放位置
    Scrubbing,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HitType {
    Start,
    Middle,
    End,
}

/// 钢琴卷帘编辑器
pub struct Editor {
    pub state: ViewState,
    pub grid_cache: canvas::Cache<crate::Renderer>,
    /// 键盘缓存（只随垂直滚动变化）
    pub keyboard_cache: canvas::Cache<crate::Renderer>,
    /// 标尺缓存（只随水平滚动变化）
    pub ruler_cache: canvas::Cache<crate::Renderer>,
    pub max_scroll_x: f32,
    pub max_scroll_y: f32,
    /// 当前鼠标在窗口中的位置
    pub cursor_position: Option<Point>,
    /// Canvas 在窗口中的偏移量（用于坐标转换）
    pub canvas_offset: Point,
    /// Canvas 尺寸（宽, 高）
    pub canvas_size: Point,

    pub notes: im::Vector<Note>,
    pub edit_state: EditState,
    pub hover_state: Option<(usize, HitType)>,
    pub pending_audio_actions: Vec<AudioAction>,

    /// 当前编辑的音轨索引
    pub current_track: usize,
    /// 按音轨存储的音符（懒加载缓存，仅保留访问过的音轨）
    pub track_notes: std::collections::HashMap<usize, im::Vector<Note>>,
    /// MIDI 文档引用（用于懒加载非当前音轨的音符，避免全量预加载导致内存翻倍）
    pub(crate) document: Option<Arc<MidiDocument>>,

    /// 洋葱皮配置
    onion_skin_config: OnionSkinConfig,

    /// 当前激活的工具
    current_tool: Tool,
    /// 选中的音符索引集合
    selected_notes: std::collections::HashSet<usize>,

    /// 协作远端用户光标信息（用户ID -> (位置, 颜色, 用户名)）
    pub remote_cursors: std::collections::HashMap<String, (Point, String, String)>,

    /// 历史记录（用于撤销/重做）
    history: history::History,

    /// 演奏指示线位置（以 tick 为单位）
    pub playback_position: f32,

    /// 音符数据是否已变化（需要更新播放管理器）
    notes_changed: bool,

    /// 自动滚动配置
    auto_scroll_config: AutoScrollConfig,

    /// 音符空间索引（惰性更新）
    pub note_index: RefCell<Option<spatial_index::NoteSpatialIndex>>,
    pub note_index_dirty: Cell<bool>,
    pub query_cache: RefCell<Vec<usize>>,

    /// 其他音轨的音符空间索引（用于洋葱皮等，懒加载）
    pub track_note_indices:
        RefCell<std::collections::HashMap<usize, spatial_index::NoteSpatialIndex>>,
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
        // 所有字段都以 pub 暴露，可以直接计算
        let note_size = std::mem::size_of::<Note>();

        // editor.notes
        let notes_len = self.notes.len();
        let notes_bytes = notes_len * note_size;

        // track_notes
        let track_notes_entries = self.track_notes.len();
        let mut track_notes_count = 0usize;
        let mut track_notes_bytes = 0usize;
        for notes in self.track_notes.values() {
            track_notes_count += notes.len();
            track_notes_bytes += notes.len() * note_size;
        }

        // document events (CompactEvent=12B, (u32,f32)=8B)
        let doc_is_some = self.document.is_some();
        let doc_event_cap = self.document.as_ref().map(|d| d.events.capacity()).unwrap_or(0);
        let doc_events_bytes = self
            .document
            .as_ref()
            .map(|doc| {
                doc.events.capacity() * 12       // CompactEvent
                    + doc.tempo_changes.capacity() * 8        // (u32, f32)
            })
            .unwrap_or(0);

        // track_note_indices
        let track_note_indices_entries = self.track_note_indices.borrow().len();

        tracing::info!(
            "[MEMORY_DEBUG] document={}, events_cap={}, notes_len={}, track_notes_entries={}, track_notes_count={}",
            doc_is_some, doc_event_cap, notes_len, track_notes_entries, track_notes_count,
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
        let mut editor = Self {
            state: ViewState::default(),
            grid_cache: canvas::Cache::new(),
            keyboard_cache: canvas::Cache::new(),
            ruler_cache: canvas::Cache::new(),
            max_scroll_x: 0.0,
            max_scroll_y: 0.0,
            cursor_position: None,
            canvas_offset: Point::new(0.0, 0.0),
            canvas_size: Point::new(0.0, 0.0),
            notes: im::Vector::new(),
            edit_state: EditState::Idle,
            hover_state: None,
            pending_audio_actions: Vec::new(),
            current_track: 0,
            track_notes: std::collections::HashMap::new(),
            document: None,
            onion_skin_config: OnionSkinConfig::new(),
            current_tool: Tool::Pointer, // 默认使用框选工具
            selected_notes: std::collections::HashSet::new(),
            remote_cursors: std::collections::HashMap::new(),
            history: history::History::new(),
            playback_position: 0.0,
            notes_changed: false,
            auto_scroll_config: AutoScrollConfig::default(),
            note_index: RefCell::new(None),
            note_index_dirty: Cell::new(true),
            query_cache: RefCell::new(Vec::new()),
            track_note_indices: RefCell::new(std::collections::HashMap::new()),
        };
        editor.max_scroll_x = editor.state.total_ticks as f32 * editor.state.zoom_x;
        editor.max_scroll_y = editor.state.visible_key_count as f32 * editor.state.zoom_y;
        editor
    }

    /// 设置当前工具
    pub fn set_tool(&mut self, tool: Tool) {
        self.current_tool = tool;
        // 切换工具时清除选中状态
        if tool != Tool::Pointer {
            self.selected_notes.clear();
        }
    }

    /// 获取当前工具
    pub fn current_tool(&self) -> Tool {
        self.current_tool
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
        self.cursor_position = position;
    }

    /// 更新 Canvas 偏移量（用于坐标转换）
    pub fn set_canvas_offset(&mut self, offset: Point) {
        self.canvas_offset = offset;
    }

    /// 更新 Canvas 尺寸
    pub fn set_canvas_size(&mut self, size: Point) {
        self.canvas_size = size;
    }

    /// 获取并清空待处理的音频动作
    pub fn take_audio_actions(&mut self) -> Vec<AudioAction> {
        let actions = std::mem::take(&mut self.pending_audio_actions);
        if !actions.is_empty() {
            tracing::debug!("Editor: 取出了 {} 个音频动作", actions.len());
        }
        actions
    }

    /// 检查音符数据是否已变化
    pub fn notes_changed(&self) -> bool {
        self.notes_changed
    }

    /// 清除音符变化标志
    pub fn clear_notes_changed(&mut self) {
        self.notes_changed = false;
    }

    /// 标记音符数据已变化
    pub fn mark_notes_changed(&mut self) {
        self.notes_changed = true;
        self.note_index_dirty.set(true);
        self.track_note_indices
            .borrow_mut()
            .remove(&self.current_track);
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
        let snapshot = history::EditorSnapshot::new(self.notes.clone(), self.current_track);
        tracing::debug!(
            "推送历史记录: {} 个音符，音轨 {}",
            snapshot.notes.len(),
            snapshot.current_track
        );
        self.history.push(snapshot);
    }

    /// Undo the last action
    pub fn undo(&mut self) -> bool {
        let current_state = history::EditorSnapshot::new(self.notes.clone(), self.current_track);
        tracing::info!(
            "尝试撤销: 当前音符数 = {}, 可撤销 = {}",
            self.notes.len(),
            self.can_undo()
        );

        if let Some(snapshot) = self.history.undo(current_state) {
            self.notes = snapshot.notes;
            self.current_track = snapshot.current_track;
            self.grid_cache.clear();
            self.mark_notes_changed();
            tracing::info!("撤销操作成功: {} 个音符", self.notes.len());
            true
        } else {
            tracing::info!("没有可撤销的操作");
            false
        }
    }

    /// Redo the last undone action
    pub fn redo(&mut self) -> bool {
        let current_state = history::EditorSnapshot::new(self.notes.clone(), self.current_track);

        if let Some(snapshot) = self.history.redo(current_state) {
            self.notes = snapshot.notes;
            self.current_track = snapshot.current_track;
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
        self.history.can_undo()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }
}
