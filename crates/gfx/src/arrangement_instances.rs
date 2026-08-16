//! 工程走带视图实例构建 — 屏幕坐标版，二分查找保性能
//!
//! 所有实例用屏幕坐标，每帧重建。音符从 MidiDocument 通过二分查找
//! 定位可见范围起点，只扫描 O(log N + K) 个事件。
//! 2026-08 单一权威源：音符一律从 `midi_doc`（MidiDocument）读取。

use crate::ArrangementNoteInstance;

/// 走带视图颜色配置
#[derive(Debug, Clone)]
pub struct ArrangementViewColors {
    pub bg: [f32; 3],
    pub lane_even: [f32; 3],
    pub lane_odd: [f32; 3],
    pub measure_line: [f32; 4],
    pub playhead: [f32; 4],
    /// 框选矩形颜色（RGB，alpha 由实例硬编码 0.15）
    pub sel_rect: [f32; 3],
}

/// 走带视图场景参数（聚合所有实例构建所需数据）
#[derive(Debug, Clone)]
pub struct ArrangementSceneParams<'a> {
    pub viewport: &'a ArrangementViewport,
    pub track_order: &'a [usize],
    pub track_colors: &'a [[f32; 3]],
    pub track_visible: &'a [bool],
    pub midi_doc: Option<&'a lumino_midi_loader::MidiDocument>,
    pub playback_position: f32,
    pub colors: &'a ArrangementViewColors,
    /// ghost 音符预览（tick_start, tick_end, track）
    pub ghost_notes: &'a [(f64, f64, usize)],
    /// 已提交的框选矩形（tick_start, tick_end, track_lo, track_hi）
    pub sel_rect: Option<(f64, f64, usize, usize)>,
    /// 拖拽中的框选矩形（tick_start, tick_end, track_lo, track_hi）
    pub drag_sel_rect: Option<(f64, f64, usize, usize)>,
    /// 拍号变化列表 (tick, 分子, 分母)，小节线按真实小节边界绘制；
    /// 空列表回退到固定 4/4（与旧行为一致）。
    pub time_signatures: &'a [(u32, u8, u8)],
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
    /// 分辨率 (Pulses Per Quarter note)
    pub ppq: u16,
}

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

        let color = if params.track_colors.is_empty() {
            [0.5, 0.5, 0.5]
        } else {
            params.track_colors[ti % params.track_colors.len()]
        };

        // Lane 背景
        let lane_y = trk_screen_y(viewport, ti) + coy;
        let c = if ti % 2 == 0 {
            colors.lane_even
        } else {
            colors.lane_odd
        };
        out.push(ArrangementNoteInstance::lane(cox, lane_y, w, lh, c));

        // 音符（2026-08 单一权威源：一律从 document 读取）
        if let Some(doc) = params.midi_doc {
            collect_notes_doc(out, doc, *tid, color, viewport, cox, lane_y);
        }
    }

    // ── 3. 小节线（按拍号变化，与标尺/真实小节边界一致）──
    for tick in crate::grid::measure_line_ticks(
        ts as u32,
        te as u32,
        viewport.ppq as u32,
        params.time_signatures,
    ) {
        let screen_x = tick_to_x(viewport, tick as f64);
        if screen_x >= cox && screen_x <= cox + w {
            out.push(ArrangementNoteInstance::grid_line(
                screen_x,
                coy,
                1.0,
                h,
                colors.measure_line,
                tick,
            ));
        }
    }

    // ── 4. 框选矩形 ──
    if let Some((t_start, t_end, track_lo, track_hi)) = params.sel_rect {
        let lh = viewport.track_height * viewport.zoom_y;
        let sx = cox + (t_start as f32) * ppu - viewport.scroll_x;
        let ex = cox + (t_end as f32) * ppu - viewport.scroll_x;
        let sy = track_lo as f32 * lh - viewport.scroll_y + coy;
        let ey = (track_hi as f32 + 1.0) * lh - viewport.scroll_y + coy;
        let min_x = sx.min(ex);
        let max_x = sx.max(ex);
        let min_y = sy.min(ey);
        let max_y = sy.max(ey);
        out.push(ArrangementNoteInstance::selection_rect(
            min_x,
            min_y,
            max_x - min_x,
            max_y - min_y,
            colors.sel_rect,
        ));
    }

    // ── 5. 拖拽框选矩形 ──
    if let Some((t_start, t_end, track_lo, track_hi)) = params.drag_sel_rect {
        let lh = viewport.track_height * viewport.zoom_y;
        let sx = cox + (t_start as f32) * ppu - viewport.scroll_x;
        let ex = cox + (t_end as f32) * ppu - viewport.scroll_x;
        let sy = track_lo as f32 * lh - viewport.scroll_y + coy;
        let ey = (track_hi as f32 + 1.0) * lh - viewport.scroll_y + coy;
        let min_x = sx.min(ex);
        let max_x = sx.max(ex);
        let min_y = sy.min(ey);
        let max_y = sy.max(ey);
        out.push(ArrangementNoteInstance::selection_rect(
            min_x,
            min_y,
            max_x - min_x,
            max_y - min_y,
            colors.sel_rect,
        ));
    }

    // ── 6. ghost 音符预览 ──
    if !params.ghost_notes.is_empty() {
        let ghost_color = [0.9, 0.9, 0.9];
        for (start, end, track) in params.ghost_notes {
            let track_i = *track;
            if track_i >= nt {
                continue;
            }
            let lane_y = trk_screen_y(viewport, track_i) + coy;
            let sx = cox + (*start as f32) * ppu - viewport.scroll_x;
            let sw = ((*end - *start) as f32) * ppu;
            let sy = lane_y + lh * 0.5 - 2.0;
            out.push(ArrangementNoteInstance::ghost_note(
                sx,
                sy,
                sw.max(2.0),
                4.0,
                ghost_color,
            ));
        }
    }

    // ── 7. 演奏指示线 ──
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

    // 分块二分范围查询（含 TICK_SEARCH_BUFFER 捕获跨范围音符）
    let ts_u = ts as u32;
    let te_u = te as u32;
    const TICK_BUF: u32 = 19200;

    let range_start = ts_u.saturating_sub(TICK_BUF);

    for note_info in notes.range(range_start, te_u + 1) {
        if note_info.start_tick > te_u {
            break;
        }
        let end_tick = note_info.end_tick();
        if end_tick < ts_u {
            continue;
        }
        // NoteInfo → 屏幕坐标，一行输出，无 active-table 开销
        let s = (note_info.start_tick as f64).max(ts);
        let e = (end_tick as f64).min(te);
        if s < e {
            let sx = cox + s as f32 * ppu - scroll_x;
            let sw = (e - s) as f32 * ppu;
            let sy = lane_y + (127.0 - note_info.key as f32) * key_h;
            out.push(ArrangementNoteInstance::note(
                sx,
                sy,
                sw.max(2.0),
                4.0,
                color,
                note_info.velocity,
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
