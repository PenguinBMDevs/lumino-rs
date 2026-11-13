//! 工具栏数据的 egui 快照（旧版 视图层删除后仅保留数据换算）。
//!
//! 录制 / 走带 / 循环 / 撤销重做 / 检测仪表盘 / 工具组 / 精度下拉 / 洋葱皮 /
//! 自动滚动 / 协作 /「更多」的绘制与输入全部在 `lumino-ui-egui::chrome::toolbar`；
//! 本模块只负责把生产状态换算成渲染好的文本（egui crate 不依赖 `lumino-note-core`）。

use lumino_note_core::midi_types::TempoPoint;

use super::{Toolbar, ToolbarPerfContext};

impl Toolbar {
    /// 生成 egui 工具栏快照（与 旧版 仪表盘使用同一套换算函数）。
    pub(crate) fn egui_snapshot(&self, perf: &ToolbarPerfContext<'_>) -> ToolbarSnapshot {
        let bpm = bpm_at_tick(perf.tempo_points, perf.playback_tick);
        let time_secs = tick_to_seconds(perf.tempo_points, perf.ppq, perf.playback_tick);
        ToolbarSnapshot {
            bpm: format!("{bpm:.1}"),
            ppq: format!("PPQ {}", perf.ppq),
            position: format_position(perf.playback_tick, perf.ppq),
            time: format_time(time_secs),
            snap: self.snap_spec.text(),
            tool: self.current_tool,
            draw_panel_open: self.tool_panel_open,
            playing: self.is_playing,
            looping: self.is_looping,
            recording: self.is_recording,
            onion: self.onion_skin_enabled,
            auto_scroll: self.auto_scroll_mode,
            height: self.height,
        }
    }
}

/// 取播放位置处的当前 BPM（取 tick 之前最近的速度点）。
fn bpm_at_tick(points: &[TempoPoint], tick: f32) -> f64 {
    let mut bpm = points.first().map(|p| p.bpm).unwrap_or(120.0);
    for p in points {
        if p.tick <= tick {
            bpm = p.bpm;
        } else {
            break;
        }
    }
    bpm
}

/// 从 tick 到 tempo 变化表换算为秒。
fn tick_to_seconds(points: &[TempoPoint], ppq: u16, tick: f32) -> f64 {
    let ppq = ppq as f64;
    if points.is_empty() {
        return (tick as f64 / ppq) / 120.0 * 60.0;
    }
    let mut seconds = 0.0_f64;
    for i in 0..points.len() {
        let seg_start = points[i].tick as f64;
        if seg_start > tick as f64 {
            break;
        }
        let seg_bpm = points[i].bpm;
        let seg_end = if i + 1 < points.len() {
            points[i + 1].tick as f64
        } else {
            tick as f64
        };
        let consume = (seg_end.min(tick as f64) - seg_start).max(0.0);
        seconds += consume / ppq / seg_bpm * 60.0;
    }
    seconds
}

/// 格式化时间为 `mm:ss.cs`（百分秒）。
fn format_time(secs: f64) -> String {
    let total_cs = (secs * 100.0).max(0.0) as u64;
    let cs = total_cs % 100;
    let total_s = total_cs / 100;
    let s = total_s % 60;
    let m = total_s / 60;
    format!("{m:02}:{s:02}.{cs:02}")
}

/// 格式化播放位置为「小节.拍」（假设 4/4 拍）。
fn format_position(tick: f32, ppq: u16) -> String {
    let ppq = ppq as f32;
    let ticks_per_bar = ppq * 4.0;
    if ticks_per_bar <= 0.0 {
        return "0.0".to_string();
    }
    let bar = (tick / ticks_per_bar).floor().max(0.0) as u32 + 1;
    let beat = (((tick % ticks_per_bar) / ppq).floor()).max(0.0) as u32 + 1;
    format!("{bar}.{beat}")
}

/// egui 工具栏快照：只读文本 + 状态位，供 `lumino-ui-egui` 渲染。
#[derive(Debug, Clone)]
pub(crate) struct ToolbarSnapshot {
    /// BPM 文本（1 位小数）。
    pub bpm: String,
    /// PPQ 文本（`PPQ {ppq}`）。
    pub ppq: String,
    /// 播放位置「小节.拍」。
    pub position: String,
    /// 播放时间 `mm:ss.cs`。
    pub time: String,
    /// 吸附规格文本。
    pub snap: String,
    /// 当前工具。
    pub tool: lumino_core::Tool,
    /// 绘制工具面板是否打开。
    pub draw_panel_open: bool,
    /// 是否正在播放。
    pub playing: bool,
    /// 是否循环播放。
    pub looping: bool,
    /// 是否正在录制。
    pub recording: bool,
    /// 洋葱皮开关。
    pub onion: bool,
    /// 自动滚动模式。
    pub auto_scroll: lumino_core::storage::config::AutoScrollMode,
    /// 工具栏高度（用户可拖拽调整，默认 72）。
    pub height: f32,
}
