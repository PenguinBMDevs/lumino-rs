//! 工程走带视图实例构建 — 屏幕坐标版，二分查找保性能
//!
//! 所有实例用屏幕坐标，每帧重建。音符从 MidiDocument 通过二分查找
//! 定位可见范围起点，只扫描 O(log N + K) 个事件。

use crate::ArrangementNoteInstance;
use lumino_core::note::Note;
use std::collections::HashMap;

type TrackNotesMap = HashMap<usize, im::Vector<Note>>;

/// 走带视图颜色配置
#[derive(Debug, Clone)]
pub struct ArrangementViewColors {
    pub bg: [f32; 3],
    pub lane_even: [f32; 3],
    pub lane_odd: [f32; 3],
    pub measure_line: [f32; 4],
    pub playhead: [f32; 4],
}

/// 走带视图场景参数（聚合所有实例构建所需数据）
#[derive(Debug, Clone)]
pub struct ArrangementSceneParams<'a> {
    pub viewport: &'a ArrangementViewport,
    pub track_order: &'a [usize],
    pub track_colors: &'a [[f32; 3]],
    pub track_visible: &'a [bool],
    pub midi_doc: Option<&'a lumino_midi_loader::MidiDocument>,
    pub track_notes: &'a TrackNotesMap,
    pub playback_position: f32,
    pub colors: &'a ArrangementViewColors,
}

/// 走带视口状态（GFX 版，纯数据，与 UI 版字段兼容）
#[derive(Debug, Clone)]
pub struct ArrangementViewport {
    /// 水平滚动（像素）
    pub scroll_x: f32,
    /// 垂直滚动（像素）
    pub scroll_y: f32,
    /// 水平缩放（像素/tick）
    pub zoom_x: f32,
    /// 垂直缩放（倍率，1.0 = 默认高度）
    pub zoom_y: f32,
    /// 每轨高度（像素）
    pub track_height: f32,
    /// Canvas 偏移（屏幕坐标）[x, y]
    pub canvas_offset: [f32; 2],
    /// Canvas 尺寸 [width, height]
    pub canvas_size: [f32; 2],
    /// 总 tick 数
    pub total_ticks: u32,
}

/// 走带视图音轨调色板（12 色，与 view.rs 保持同步）
pub const ARRANGEMENT_PALETTE: [[f32; 3]; 12] = [
    [0.90, 0.30, 0.30], // 红
    [0.30, 0.70, 0.30], // 绿
    [0.30, 0.50, 0.90], // 蓝
    [0.90, 0.70, 0.20], // 黄
    [0.70, 0.30, 0.80], // 紫
    [0.20, 0.80, 0.80], // 青
    [0.90, 0.50, 0.50], // 粉红
    [0.50, 0.90, 0.30], // lime
    [0.30, 0.30, 0.70], // 深蓝
    [0.90, 0.80, 0.30], // 橙
    [0.60, 0.40, 0.20], // 棕
    [0.50, 0.50, 0.50], // 灰
];

/// 构建全部实例（背景 + lane + 网格线 + 音符 + 演奏指示线）
/// 屏幕坐标，每帧重建，二分查找加速 MidiDocument 音符读取
pub fn build_arrangement_all(
    out: &mut Vec<ArrangementNoteInstance>,
    params: &ArrangementSceneParams<'_>,
) {
    let viewport = params.viewport;
    let colors = params.colors;
    let w = viewport.canvas_size[0];
    let h = viewport.canvas_size[1];
    let lh = viewport.track_height * viewport.zoom_y;
    let ppu = viewport.zoom_x.max(0.001);
    let nt = params.track_order.len();
    let cox = viewport.canvas_offset[0];
    let coy = viewport.canvas_offset[1];

    // 可见 tick 范围（屏幕坐标模式下，这是所有元素的计算基准）
    let ts = (viewport.scroll_x / ppu) as f64;
    let te = ((viewport.scroll_x + w) / ppu) as f64;

    // ── 1. 背景 ──
    out.push(ArrangementNoteInstance::background(
        cox, coy, w, h, colors.bg,
    ));

    if nt == 0 {
        return;
    }

    let (tf, tl) = visible_trk_range(viewport, h, nt);

    // ── 2. Lane + 音符 ──
    for (ti, tid) in params.track_order.iter().enumerate() {
        if ti < tf || ti >= tl {
            continue;
        }
        if !params.track_visible.get(ti).copied().unwrap_or(true) {
            continue;
        }

        let color = params
            .track_colors
            .get(ti)
            .copied()
            .unwrap_or([0.5, 0.5, 0.5]);

        // Lane 背景
        let lane_y = trk_screen_y(viewport, ti) + coy;
        let c = if ti % 2 == 0 {
            colors.lane_even
        } else {
            colors.lane_odd
        };
        out.push(ArrangementNoteInstance::lane(cox, lane_y, w, lh, c));

        // 音符
        if let Some(notes) = params.track_notes.get(tid) {
            collect_notes_cache(out, notes, color, viewport, cox, lane_y);
        } else if let Some(doc) = params.midi_doc {
            collect_notes_doc(out, doc, *tid, color, viewport, cox, lane_y);
        }
    }

    // ── 3. 小节线 ──
    let tpb = 480.0_f64 * 4.0;
    for bar in ((ts / tpb).floor() as i32).max(0)..=(te / tpb).ceil() as i32 {
        let tick = bar as f64 * tpb;
        let x = tick_to_x(viewport, tick);
        if x >= cox && x <= cox + w {
            out.push(ArrangementNoteInstance::grid_line(
                x,
                coy,
                1.0,
                h,
                colors.measure_line,
                tick as u32,
            ));
        }
    }

    // ── 4. 演奏指示线 ──
    if params.playback_position > 0.0 {
        let cx = tick_to_x(viewport, params.playback_position as f64);
        if cx >= cox && cx <= cox + w {
            out.push(ArrangementNoteInstance::playhead(
                cx,
                coy,
                2.0,
                h,
                colors.playhead,
            ));
        }
    }
}

/// Collect arrangement instances from raw data (no UI dependency).
pub fn collect_arrangement_instances(
    params: &ArrangementSceneParams<'_>,
) -> Vec<ArrangementNoteInstance> {
    let mut instances = Vec::new();
    build_arrangement_all(&mut instances, params);
    instances
}

// ─── 音符读取 ──────────────────────────────────────────────

fn collect_notes_cache(
    out: &mut Vec<ArrangementNoteInstance>,
    notes: &im::Vector<Note>,
    color: [f32; 3],
    viewport: &ArrangementViewport,
    cox: f32,
    lane_y: f32,
) {
    let ppu = viewport.zoom_x.max(0.001);
    let w = viewport.canvas_size[0];
    let ts = (viewport.scroll_x / ppu) as f64;
    let te = ((viewport.scroll_x + w) / ppu) as f64;
    let lh = viewport.track_height * viewport.zoom_y;
    let key_h = lh / 128.0;
    let scroll_x = viewport.scroll_x;

    for n in notes {
        let s = n.tick as f64;
        let e = (n.tick + n.length) as f64;
        if s > te || e < ts {
            continue;
        }
        let sx = cox + s as f32 * ppu - scroll_x;
        let sw = (e - s) as f32 * ppu;
        let sy = lane_y + (127.0 - n.key as f32) * key_h;
        out.push(ArrangementNoteInstance::note(
            sx,
            sy,
            sw.max(2.0),
            4.0,
            color,
            n.velocity,
        ));
    }
}

fn collect_notes_doc(
    out: &mut Vec<ArrangementNoteInstance>,
    doc: &lumino_midi_loader::MidiDocument,
    tid: usize,
    color: [f32; 3],
    viewport: &ArrangementViewport,
    cox: f32,
    lane_y: f32,
) {
    let ppu = viewport.zoom_x.max(0.001);
    let w = viewport.canvas_size[0];
    let ts = (viewport.scroll_x / ppu) as f64;
    let te = ((viewport.scroll_x + w) / ppu) as f64;
    let lh = viewport.track_height * viewport.zoom_y;
    let key_h = lh / 128.0;
    let scroll_x = viewport.scroll_x;

    let notes = doc.track_notes(tid);
    if notes.is_empty() {
        return;
    }

    // 二分查找起点（退 TICK_SEARCH_BUFFER 以捕获跨范围音符）
    let ts_u = ts as u32;
    let te_u = te as u32;
    const TICK_BUF: u32 = 19200;

    let search_start = notes.partition_point(|n| n.start_tick < ts_u.saturating_sub(TICK_BUF));
    let slice = &notes[search_start..];

    for n in slice {
        if n.start_tick > te_u {
            break;
        }
        let end_tick = n.end_tick();
        if end_tick < ts_u {
            continue;
        }
        // NoteInfo → 屏幕坐标，一行输出，无 active-table 开销
        let s = (n.start_tick as f64).max(ts);
        let e = (end_tick as f64).min(te);
        if s < e {
            let sx = cox + s as f32 * ppu - scroll_x;
            let sw = (e - s) as f32 * ppu;
            let sy = lane_y + (127.0 - n.key as f32) * key_h;
            out.push(ArrangementNoteInstance::note(
                sx,
                sy,
                sw.max(2.0),
                4.0,
                color,
                n.velocity,
            ));
        }
    }
}

// ─── 辅助 ──────────────────────────────────────────────

fn trk_screen_y(viewport: &ArrangementViewport, i: usize) -> f32 {
    i as f32 * viewport.track_height * viewport.zoom_y - viewport.scroll_y
}

fn visible_trk_range(viewport: &ArrangementViewport, h: f32, nt: usize) -> (usize, usize) {
    let effective_track_height = viewport.track_height * viewport.zoom_y;
    let f =
        ((viewport.scroll_y / effective_track_height).floor() as usize).min(nt.saturating_sub(1));
    let c = (h / effective_track_height).ceil() as usize + 1;
    (f, (f + c).min(nt))
}

fn tick_to_x(viewport: &ArrangementViewport, tick: f64) -> f32 {
    viewport.canvas_offset[0] + (tick as f32 * viewport.zoom_x) - viewport.scroll_x
}
