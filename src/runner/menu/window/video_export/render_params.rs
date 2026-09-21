//! 视频导出帧渲染参数构建
//!
//! 单一权威飞行格式：所有 GPU 模式统一产出 `note_instances`（`NoteInstance`），
//! 瀑布流 / 3D 所需的派生数据由渲染线程按需换算，不再各存一份。
//! 将 RenderParams 构建逻辑按渲染模式拆分为子模块：
//! - `note_rectangle`：Lumino卷帘（NoteRectangle 传统钢琴卷帘矩形）
//! - `waterfall`：瀑布流（产出 note_instances + 瀑布流 uniforms）
//! - `miditrail`：3D MIDI 轨迹（产出 note_instances + 3D uniforms）

use lumino_gfx::NoteInstance;
use lumino_message::events::window::video::MiditrailViewMode;
use lumino_message::events::window::video::RenderMode;
use lumino_midi_loader::MidiDocument;

/// 视频导出每帧可见音符的临时数据结构
#[derive(Clone)]
pub struct SortableNote {
    pub key: u8,
    pub start_tick: u32,
    pub length: u32,
    pub track_idx: u16,
}

/// 导出参数构建暂存（首帧全量收集 + 排序复用，避免每帧大分配）。
///
/// 历史注：曾含滑动窗口游标（`cursors/last_tick`，逐帧窗口收集增量推进）；
/// GPU cull 接管窗口过滤后逐帧收集已删除（首帧全量 + 稳态跳过），游标随之删除，
/// 仅保留排序暂存（`collect_all_notes` 后的计数排序仍需它）。
#[derive(Default)]
pub struct WindowCollectState {
    /// `sort_visible_notes` 计数排序暂存（常驻复用，消首帧 N×SortableNote 分配）。
    sort_scratch: Vec<SortableNote>,
}

/// `build_video_export_render_params` 入参（替代 12 个位置参数，消除 `too_many_arguments`）
pub struct RenderParamsInput<'a> {
    pub width: u32,
    pub height: u32,
    pub tick: u32,
    pub document: &'a MidiDocument,
    pub ppq: u32,
    pub key_count: u16,
    pub render_mode: RenderMode,
    pub waterfall_scroll_speed: f32,
    pub miditrail_z_far: f32,
    pub miditrail_view_mode: MiditrailViewMode,
    pub miditrail_normal_speed: f32,
    pub miditrail_top_speed: f32,
    pub miditrail_3d_notes: bool,
    pub fps: f32,
    pub visible_notes: &'a mut Vec<SortableNote>,
    pub note_instances_out: &'a mut Vec<NoteInstance>,
    /// 首帧全量收集（全文档音符一次上传）；后续帧跳过收集，渲染线程复用 GPU 常驻数据。
    pub collect_all: bool,
    /// 排序暂存（首帧全量排序复用，见 `WindowCollectState`）。
    pub window_state: &'a mut WindowCollectState,
}

/// NoteRectangle 模式 `build_note_rectangle_params_from_visible` 入参
pub(crate) struct NoteRectangleParamsInput<'a> {
    pub width: u32,
    pub height: u32,
    pub tick: u32,
    pub visible_notes: &'a mut Vec<SortableNote>,
    pub note_instances_out: &'a mut Vec<NoteInstance>,
    pub ppq: u32,
    pub time_signatures: &'a [(u32, u8, u8)],
}

/// NoteRectangle 模式 `build_note_rectangle_render_params` 入参（内部分发用）
pub(crate) struct NoteRectangleRenderInput<'a> {
    pub width: u32,
    pub height: u32,
    pub tick: u32,
    pub document: &'a MidiDocument,
    pub ppq: u32,
    pub visible_notes: &'a mut Vec<SortableNote>,
    pub note_instances_out: &'a mut Vec<NoteInstance>,
    pub collect_all: bool,
}

/// 瀑布流模式 `build_waterfall_render_params` 入参（内部分发用）
pub(crate) struct WaterfallRenderInput<'a> {
    pub width: u32,
    pub height: u32,
    pub tick: u32,
    pub document: &'a MidiDocument,
    pub ppq: u32,
    pub key_count: u16,
    pub waterfall_scroll_speed: f32,
    pub visible_notes: &'a mut Vec<SortableNote>,
    pub note_instances_out: &'a mut Vec<NoteInstance>,
    pub window_state: &'a mut WindowCollectState,
    /// 首帧全量（导出常驻一次上传，窗口过滤走 GPU cull）；后续帧跳过收集只发 uniforms。
    pub collect_all: bool,
}

/// MIDITrail 模式 `build_miditrail_render_params` 入参（内部分发用）
pub(crate) struct MiditrailRenderInput<'a> {
    pub width: u32,
    pub height: u32,
    pub tick: u32,
    pub document: &'a MidiDocument,
    pub ppq: u32,
    pub key_count: u16,
    pub miditrail_speed: f32,
    pub miditrail_view_mode: MiditrailViewMode,
    pub miditrail_z_far: f32,
    pub miditrail_3d_notes: bool,
    pub fps: f32,
    pub visible_notes: &'a mut Vec<SortableNote>,
    pub note_instances_out: &'a mut Vec<NoteInstance>,
    pub window_state: &'a mut WindowCollectState,
    /// 首帧全量（导出常驻一次上传，窗口过滤走 GPU cull）；后续帧跳过收集只发 uniforms。
    pub collect_all: bool,
}

mod build;
mod helpers;
mod miditrail;
mod note_rectangle;
mod note_rectangle_params;
mod waterfall;

#[cfg(test)]
mod tests;

/// 按视图解析 MIDITrail 滚动速度（Normal/Top 各自独立，防互相污染）。
fn resolve_miditrail_speed(view_mode: MiditrailViewMode, normal_speed: f32, top_speed: f32) -> f32 {
    if view_mode.is_top() {
        top_speed.max(0.1)
    } else {
        normal_speed.max(0.1)
    }
}

pub use build::build_video_export_render_params;
pub(crate) use helpers::collect_all_notes;
pub(crate) use helpers::diag_window_collect;
#[cfg(test)]
pub(crate) use helpers::note_search_bounds;
pub(crate) use helpers::pack_note_instances;
pub(crate) use helpers::sort_visible_notes;
pub(crate) use miditrail::build_miditrail_render_params;
pub(crate) use note_rectangle::build_note_rectangle_render_params;
pub(crate) use note_rectangle_params::build_note_rectangle_params_from_visible;
pub(crate) use waterfall::build_waterfall_render_params;
