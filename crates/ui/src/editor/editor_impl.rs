use crate::editor::note::Note;
use crate::editor::{EditState, HitType, OnionSkinConfig, OnionSkinViewportCache, ViewState};
use crate::message::AudioAction;
use crate::toolbar::Tool;
use iced_core::Point;
use iced_widget::canvas;
use lumino_core::storage::config::AutoScrollConfig;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

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
    pub track_notes: HashMap<usize, im::Vector<Note>>,

    /// 洋葱皮配置
    pub(super) onion_skin_config: OnionSkinConfig,

    /// 当前激活的工具
    pub(super) current_tool: Tool,
    /// 选中的音符索引集合
    pub(super) selected_notes: HashSet<usize>,

    /// 协作远端用户光标信息（用户ID -> (位置, 颜色, 用户名)）
    pub remote_cursors: HashMap<String, (Point, String, String)>,

    /// 历史记录（用于撤销/重做）
    pub(super) history: super::history::History,

    /// 演奏指示线位置（以 tick 为单位）
    pub playback_position: f32,

    /// 音符数据是否已变化（需要更新播放管理器）
    pub(super) notes_changed: bool,

    /// 自动滚动配置
    pub(super) auto_scroll_config: AutoScrollConfig,

    /// 音符空间索引（惰性更新）
    pub note_index: RefCell<Option<super::spatial_index::NoteSpatialIndex>>,
    pub note_index_dirty: Cell<bool>,
    pub query_cache: RefCell<Vec<usize>>,

    /// 其他音轨的音符空间索引（用于洋葱皮等，懒加载）
    pub track_note_indices: RefCell<HashMap<usize, super::spatial_index::NoteSpatialIndex>>,
}

impl Editor {
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
            track_notes: HashMap::new(),
            onion_skin_config: OnionSkinConfig::new(),
            current_tool: Tool::Pointer,
            selected_notes: HashSet::new(),
            remote_cursors: HashMap::new(),
            history: super::history::History::new(),
            playback_position: 0.0,
            notes_changed: false,
            auto_scroll_config: AutoScrollConfig::default(),
            note_index: RefCell::new(None),
            note_index_dirty: Cell::new(true),
            query_cache: RefCell::new(Vec::new()),
            track_note_indices: RefCell::new(HashMap::new()),
        };
        editor.max_scroll_x = editor.state.total_ticks as f32 * editor.state.zoom_x;
        editor.max_scroll_y = editor.state.visible_key_count as f32 * editor.state.zoom_y;
        editor
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
