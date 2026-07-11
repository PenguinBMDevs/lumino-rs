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

/// 洋葱皮音轨调色板（按音轨索引循环取色，alpha 固定 255）
///
/// 与 `host.rs` 中的 `onion_track_color` 保持一致。
/// 8 色调色板循环使用，覆盖多轨场景。
pub fn onion_track_color(track_idx: usize) -> [u8; 4] {
    const PALETTE: [[u8; 4]; 8] = [
        [200, 80, 80, 255],
        [80, 200, 120, 255],
        [80, 120, 220, 255],
        [220, 200, 80, 255],
        [200, 100, 200, 255],
        [80, 200, 200, 255],
        [240, 150, 80, 255],
        [180, 180, 180, 255],
    ];
    PALETTE[track_idx % PALETTE.len()]
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
    pub(crate) note_index: RefCell<Option<spatial_index::NoteSpatialIndex>>,
    pub(crate) note_index_dirty: Cell<bool>,
    pub(crate) query_cache: RefCell<Vec<usize>>,
    /// 其他音轨的音符空间索引（用于洋葱皮等，懒加载）
    pub(crate) track_note_indices:
        RefCell<std::collections::HashMap<usize, spatial_index::NoteSpatialIndex>>,
}

/// 钢琴卷帘编辑器
pub struct Editor {
    pub(crate) grid_cache: canvas::Cache<crate::Renderer>,
    /// 键盘缓存（只随垂直滚动变化）
    pub(crate) keyboard_cache: canvas::Cache<crate::Renderer>,
    /// 标尺缓存（只随水平滚动变化）
    pub(crate) ruler_cache: canvas::Cache<crate::Renderer>,

    /// 空间索引状态（音符索引、查询缓存等）
    pub(crate) spatial: SpatialIndexState,

    /// 协作远端用户光标信息（用户ID -> (位置, 颜色, 用户名)）
    pub(crate) remote_cursors: std::collections::HashMap<String, (Point, String, String)>,

    /// 演奏指示线位置（以 tick 为单位）
    pub(crate) playback_position: f32,

    /// 播放期间琴键洋葱皮颜色（key → RGBA 颜色）
    /// 使用固定大小数组替代HashMap，直接索引O(1)，避免hash计算开销
    /// 索引 = key (0-255)，值 = [R, G, B, A]，全零表示无颜色
    pub(crate) playback_key_colors: [u8; 1024],

    /// 播放时键盘颜色指示是否启用（默认关闭，节省内存和CPU）
    pub(crate) playback_key_colors_enabled: bool,

    /// 循环区域状态
    pub(crate) loop_range: Option<grid::LoopRange>,

    /// 音符数据是否已变化（需要更新播放管理器）
    pub(crate) notes_changed: bool,

    /// 统一状态管理
    pub editor_state: editor_state::EditorState,

    /// 力度编辑面板
    pub(crate) velocity_panel: velocity::VelocityPanel,

    /// 框选框的动画显示状态（用于弹簧物理动画）
    pub(crate) selection_box_anim: Cell<Option<SelectionBoxAnimState>>,
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
    /// 空间索引追踪条目数
    pub track_note_indices_entries: usize,
}
