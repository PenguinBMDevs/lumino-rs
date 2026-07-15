// 从依赖 crate 重新导出编辑器内部常用的类型，保持模块拆分前 `crate::` 引用兼容
pub use lumino_event as event;
pub use lumino_ui_constants as constants;
pub use lumino_ui_core::message;
pub use lumino_ui_core::sidebar_event as sidebar;
pub use lumino_ui_core::theme;
pub use lumino_ui_core::{Element, Message, Renderer, Theme};

pub mod arrangement;
pub mod editor_state;
pub mod grid;
pub mod history;
pub mod note;
pub mod recording;
pub mod scrollbar_widget;
pub mod smooth_scroll;
pub mod spatial_index;
pub mod tempo_envelope;
pub mod velocity;

// 子模块
mod auto_scroll;
mod clipboard;
mod coords;
mod drag;
mod interaction;
mod note_flip;
mod note_ops;
mod note_split_glue;
mod note_transform;
mod note_transpose;
mod rendering;
mod scroll;
mod settings;
mod track;

#[cfg(test)]
mod tests {
    mod drawing;
    mod interaction;
    mod keyboard_colors_test;
    mod scroll;
    mod state;
}

use iced_core::Point;
use iced_widget::canvas;
use std::cell::{Cell, RefCell};

// 统一从 editor_state 导入（重构迁移）
pub use editor_state::{EditState, HitType, SelectionHitType, ViewState};
pub use note::Note;

mod impls;

/// 洋葱皮音轨调色板（按音轨索引循环取色）
///
/// 从当前调色板的第二个颜色开始取色（第一个颜色保留给主音轨音符）。
pub fn onion_track_color(track_idx: usize) -> [u8; 4] {
    lumino_core::palette::onion_track_color(track_idx)
}

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

/// 空间索引状态（从 Editor 提取，减少字段数）
#[derive(Debug)]
pub struct SpatialIndexState {
    /// 音符空间索引（惰性更新）
    pub note_index: RefCell<Option<spatial_index::NoteSpatialIndex>>,
    pub note_index_dirty: Cell<bool>,
    pub query_cache: RefCell<Vec<usize>>,
}

/// 钢琴卷帘编辑器
pub struct Editor {
    pub grid_cache: canvas::Cache<lumino_ui_core::Renderer>,
    /// 键盘缓存（只随垂直滚动变化）
    pub keyboard_cache: canvas::Cache<lumino_ui_core::Renderer>,
    /// 标尺缓存（只随水平滚动变化）
    pub ruler_cache: canvas::Cache<lumino_ui_core::Renderer>,

    /// 空间索引状态（音符索引、查询缓存等）
    pub spatial: SpatialIndexState,

    /// 协作远端用户光标信息（用户ID -> (位置, 颜色, 用户名)）
    pub remote_cursors: std::collections::HashMap<String, (Point, String, String)>,

    /// 演奏指示线位置（以 tick 为单位）
    pub playback_position: f32,

    /// 播放期间琴键洋葱皮颜色（key → RGBA 颜色）
    /// 使用固定大小数组替代HashMap，直接索引O(1)，避免hash计算开销
    /// 索引 = key (0-255)，值 = [R, G, B, A]，全零表示无颜色
    pub(crate) playback_key_colors: [u8; 1024],

    /// 播放时键盘颜色指示是否启用（默认关闭，节省内存和CPU）
    pub(crate) playback_key_colors_enabled: bool,

    /// 循环区域状态
    pub loop_range: Option<grid::LoopRange>,

    /// 音符数据是否已变化（需要更新播放管理器）
    pub(crate) notes_changed: bool,

    /// 统一状态管理
    pub editor_state: editor_state::EditorState,

    /// 力度编辑面板
    pub velocity_panel: velocity::VelocityPanel,

    /// 框选框的动画显示状态（用于弹簧物理动画）
    pub selection_box_anim: Cell<Option<SelectionBoxAnimState>>,
}

/// 框选框弹簧动画状态
#[derive(Debug, Clone, Copy)]
pub struct SelectionBoxAnimState {
    /// 起点的屏幕坐标（固定）
    pub start_pos: Point,
    /// 当前动画显示的终点坐标（弹簧末端）
    pub current_pos: Point,
    /// 当前速度（用于弹簧物理）
    pub velocity: Point,
    /// 上一次吸附的目标 tick（用于判断是否需要更新弹簧目标）
    pub snapped_tick: f32,
    /// 上一次吸附的目标 key
    pub snapped_key: u16,
    /// 弹簧是否已收敛到目标位置（AnimationTick 循环用）
    pub converged: bool,
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
}
