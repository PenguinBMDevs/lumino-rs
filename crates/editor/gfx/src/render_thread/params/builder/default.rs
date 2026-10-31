//! [`RenderParamsBuilder`] 的 `Default` 实现。
//!
//! 默认值**单一来源**：逐字段从 [`RenderParams::default`] 拷贝，避免与主类型
//! 手写默认值漂移（新增字段只需改主类型一处）。
//!
//! 独立成文件的原因：`builder.rs` 已接近 400 行上限（AGENTS.md 文件长度红线），
//! 把这一整块「纯字段拷贝」从 `builder.rs` 拆出，不改变任何行为。

use super::RenderParamsBuilder;
use crate::render_thread::params::RenderParams;

impl Default for RenderParamsBuilder {
    fn default() -> Self {
        let base = RenderParams::default();
        Self {
            viewport_size: base.viewport_size,
            logical_size: base.logical_size,
            scale_factor: base.scale_factor,
            scroll: base.scroll,
            zoom: base.zoom,
            keyboard_width: base.keyboard_width,
            ruler_height: base.ruler_height,
            background_color: base.background_color,
            color_bg: base.color_bg,
            color_bg_black_key: base.color_bg_black_key,
            color_bar: base.color_bar,
            color_beat: base.color_beat,
            color_half_beat: base.color_half_beat,
            color_grid: base.color_grid,
            color_key_line: base.color_key_line,
            grid_instances: base.grid_instances,
            ruler_instances: base.ruler_instances,
            ppq: base.ppq,
            max_key_index: base.max_key_index,
            is_arrangement_mode: base.is_arrangement_mode,
            arrangement_overlay_instances: base.arrangement_overlay_instances,
            arrangement_overlay_back_len: base.arrangement_overlay_back_len,
            arrangement_track_order: base.arrangement_track_order,
            arrangement_track_visible: base.arrangement_track_visible,
            arrangement_lane_index: base.arrangement_lane_index,
            arrangement_note_uniform: base.arrangement_note_uniform,
            arrangement_uniform: base.arrangement_uniform,
            cc_bar_instances: base.cc_bar_instances,
            canvas_offset: base.canvas_offset,
            canvas_size: base.canvas_size,
            velocity_panel_rect: base.velocity_panel_rect,
            time_signatures: base.time_signatures,
            is_waterfall_mode: base.is_waterfall_mode,
            waterfall_speed: base.waterfall_speed,
            waterfall_current_tick: base.waterfall_current_tick,
            miditrail_enabled: base.miditrail_enabled,
            miditrail_speed: base.miditrail_speed,
            miditrail_view_mode: base.miditrail_view_mode,
            miditrail_current_tick: base.miditrail_current_tick,
            miditrail_z_far: base.miditrail_z_far,
            miditrail_3d_notes: base.miditrail_3d_notes,
            miditrail_ticks_per_second: base.miditrail_ticks_per_second,
            fps: base.fps,
            skip_scene_render: base.skip_scene_render,
            is_vertical_roll: base.is_vertical_roll,
            content_dirty: base.content_dirty,
            subpixel_note_mode: base.subpixel_note_mode,
            vs_cull_mode: base.vs_cull_mode,
            cpu_onion_frame: base.cpu_onion_frame,
            cpu_onion_active: base.cpu_onion_active,
        }
    }
}
