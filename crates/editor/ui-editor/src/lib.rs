//! 钢琴卷帘用户界面编辑器 crate。
//!
//! 本 crate 提供基于 iced 的钢琴卷帘编辑器主体，涵盖网格、音符编辑、
//! 滚动、缩放、力度编辑、滚动条控件等 UI 能力，并整合
//! [`lumino_editor_state`] 与底层文档状态模块。

// 从依赖 crate 重新导出编辑器内部常用的类型，保持模块拆分前 `crate::` 引用兼容
pub use lumino_message::events as event;
pub use lumino_ui_core::constants;
pub use lumino_ui_core::message;
pub use lumino_ui_core::sidebar_event as sidebar;
pub use lumino_ui_core::theme;
pub use lumino_ui_core::{Element, Message, Renderer, Theme};

pub mod arrangement;
pub mod context_menu;
pub mod editor_state;
pub mod grid;
pub mod history;
pub mod note;
pub mod recording;
pub mod scrollbar_widget;
pub mod smooth_scroll;
pub mod spatial_index;
pub mod velocity;
pub mod zoom;

// 子模块
mod arrangement_ops;
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
mod puffin_profiler;
mod rendering;
mod scroll;
mod settings;
mod track;

#[cfg(test)]
mod tests {
    mod arrangement_track_mapping;
    mod drawing;
    mod flow;
    mod ghost;
    mod interaction;
    mod interception;
    mod keyboard_colors_test;
    mod pending_copy;
    mod pending_drag;
    mod pressed_priority;
    mod preview_sequence;
    mod scroll;
    mod selection_precision;
    mod state;
    pub(crate) mod test_helpers;
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
/// 从当前调色板的第一个颜色开始取色。
pub fn onion_track_color(track_idx: usize) -> [u8; 4] {
    lumino_extras::palette::onion_track_color(track_idx)
}

/// 缓存失效标志位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheInvalidation(u8);

impl CacheInvalidation {
    /// 无缓存失效
    pub const NONE: Self = Self(0);
    /// 网格缓存失效
    pub const GRID: Self = Self(1 << 0);
    /// 键盘缓存失效
    pub const KEYBOARD: Self = Self(1 << 1);
    /// 标尺缓存失效
    pub const RULER: Self = Self(1 << 2);
    /// 全部缓存失效
    pub const ALL: Self = Self(0b111);
}

/// 空间索引状态（从 Editor 提取，减少字段数）
#[derive(Debug)]
pub struct SpatialIndexState {
    /// 音符空间索引（惰性更新）
    pub note_index: RefCell<Option<spatial_index::NoteSpatialIndex>>,
    /// 空间索引是否已过期（需重建）
    pub note_index_dirty: Cell<bool>,
    /// 空间索引查询结果的缓存
    pub query_cache: RefCell<Vec<usize>>,
}

/// 钢琴卷帘编辑器
pub struct Editor {
    /// 网格绘制缓存
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

    /// 批量拖动待提交状态（ghost 方案 - 延迟提交）
    ///
    /// `DraggingSelection` 松手后不立即 apply 到 document（音符唯一权威），
    /// 而是保存到此字段。
    /// 用户点击空白处取消框选时才真正 apply，避免连续拖动期间触发空间索引重建。
    ///
    /// **累积模式**：再次拖动同一选区时，新 delta 会叠加到此字段的 delta 上。
    /// 渲染时 ghost 位置 = note + pending_drag_state.delta + drag_state.delta。
    ///
    /// `None` 表示无待提交的拖动；`Some(drag_state)` 表示有待提交的累积偏移。
    pub(crate) pending_drag_state: Option<lumino_editor_state::DragState>,

    /// 批量复制待提交状态（Ctrl+拖动 ghost 方案）
    ///
    /// `DraggingSelectionCopy` **松手即提交**：副本立即 `batch_insert_notes`
    /// 写入 document（音符唯一权威），副本真实化并只选中副本。
    /// 本字段仅在复制拖动**过程中**（`DraggingSelectionCopy`）与提交失败
    /// 兜底（`commit_pending_copy` 内部）短暂存在；正常松手后即清空。
    ///
    /// `None` 表示无待提交的复制；`Some(drag_state)` 表示有待写入的副本偏移。
    pub(crate) pending_copy_drag_state: Option<lumino_editor_state::DragState>,

    /// 统一状态管理
    pub editor_state: editor_state::EditorState,

    /// 力度编辑面板
    pub velocity_panel: velocity::VelocityPanel,

    /// 框选框的动画显示状态（用于弹簧物理动画）
    pub selection_box_anim: Cell<Option<SelectionBoxAnimState>>,

    /// 上一帧框选矩形边界（raw），用于计算增量 delta。
    ///
    /// 元组: (min_tick, max_tick, min_key, max_key)
    /// 存储上一帧精确的 raw 边界，配合 `rect_subtract` 计算新增/减少的薄条区域，
    /// 仅对 delta 区域执行 R-tree 查询，避免每帧 O(N) 全量重建。
    /// None = 不在 Selecting 状态 / 未初始化。
    pub(crate) cached_selection_bounds: Cell<Option<(f32, f32, u16, u16)>>,

    /// 钢琴卷帘右键上下文菜单状态
    pub context_menu: context_menu::PianoRollContextMenuState,

    /// 选择框边界缓存（raw 坐标），增量维护，避免每帧 O(N) 全量扫描。
    /// 每次选中/取消选中音符时增量更新，仅 ghost 路径（拖拽中）需实时计算。
    /// 元组: (min_tick, max_tick_end, max_key, min_key)
    pub(crate) selected_bounds: Cell<Option<(f32, f32, u16, u16)>>,

    /// 播放键色增量扫描状态——避免每帧 O(N) 全量扫描导致的线性性能退化
    pub(crate) playback_scan_state: impls::PlaybackScanState,

    /// Ctrl 键按下状态（窗口级 `CtrlKeyChanged` 消息驱动，可靠通道）
    ///
    /// 与 `GridInteractionState.control_pressed`（iced canvas 内事件，可能因焦点
    /// 问题不送达）互为兜底：ruler/键盘区的 Ctrl+滚轮缩放以此字段为准。
    ctrl_pressed: bool,

    /// 远端用户选择集合（用于协作高亮 + first-writer-wins 冲突判定）
    ///
    /// key = 远端用户 ID，value = 该用户的选择指纹与时间戳。
    pub remote_selections: std::collections::HashMap<String, RemoteSelectionSet>,

    /// 本地当前选择的时间戳（ms）
    ///
    /// 框选完成时由 `emit_local_selection_changed(true)` 写入，用于冲突判定：
    /// 提交本地编辑前比对远端选择时间戳，远端更早则本地让行。
    pub(crate) local_selection_timestamp: Option<u64>,

    /// 本地当前选择的指纹（track, tick, key, length）
    ///
    /// 与 `local_selection_timestamp` 同步写入/清空，供冲突判定复用，
    /// 避免在提交热路径重新扫描选中集合。
    pub(crate) local_selection_fingerprints: Vec<(usize, f32, u16, f32)>,
}

/// 远端用户选择集合
///
/// 由 `RemoteSelection` 协作事件解析填充，用于在钢琴卷帘上绘制按用户着色的
/// 选择高亮，并为 first-writer-wins 冲突判定提供远端选择指纹与时间戳。
#[derive(Debug, Clone)]
pub struct RemoteSelectionSet {
    /// 选择时间戳（ms，first-writer-wins 用）
    pub timestamp: u64,
    /// 选择指纹列表（track, tick, key, length）
    pub fingerprints: Vec<(usize, f32, u16, f32)>,
    /// 远端用户颜色（hex 字符串；空时由接收方按 user_id 派生）
    pub color: String,
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
    /// document 中全部音轨的音符总量
    pub track_notes_count: usize,
    /// document 的音轨条数
    pub track_notes_entries: usize,
    /// document 中音符数据的实际占用（cap × sizeof(NoteEvent)，
    /// 2026-08-15 起为唯一音符字节统计口径；`notes` / `track_notes`
    /// 冗余缓存已删除，不再重复统计同一份数据）
    pub document_events_bytes: usize,
}
