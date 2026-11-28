//! 工程走带视图实例构建
//!
//! 本模块只负责**覆盖层**实例（背景/lane/网格/框选/ghost/演奏指示线），每帧重建，
//! 实例数极少，开销可忽略。音符层不再在此构建——走带直接复用钢琴卷帘常驻 GPU
//! 音符缓冲（`onion_skin.gpu_note_buffer`），由 `arrangement_note.wgsl` 着色器按
//! 泳道映射并裁剪，彻底消除每帧音符实例重建与第二份 GPU 显存。
//! 2026-08 单一权威源：音符一律从 `midi_doc`（MidiDocument）经 GPU 缓冲读取。

use crate::ArrangementNoteInstance;
use crate::arrangement_renderer::ARRANGEMENT_NOTE_BORDER_DARKEN_FACTOR;

/// 走带视图颜色配置
#[derive(Debug, Clone)]
pub struct ArrangementViewColors {
    /// 画布背景色 (RGB)
    pub bg: [f32; 3],
    /// 偶数音轨 lane 背景色 (RGB)
    pub lane_even: [f32; 3],
    /// 奇数音轨 lane 背景色 (RGB)
    pub lane_odd: [f32; 3],
    /// 小节线颜色 (RGBA)
    pub measure_line: [f32; 4],
    /// 演奏指示线颜色 (RGBA)
    pub playhead: [f32; 4],
    /// 框选矩形颜色（RGB，alpha 由实例硬编码 0.15）
    pub sel_rect: [f32; 3],
}

/// 走带视图场景参数（聚合所有实例构建所需数据）
#[derive(Debug, Clone)]
pub struct ArrangementSceneParams<'a> {
    /// 当前走带视口（滚动、缩放、画布尺寸等）
    pub viewport: &'a ArrangementViewport,
    /// 音轨绘制顺序（元素为逻辑轨道 id），决定 lane 与音符的上/下层叠顺序
    pub track_order: &'a [usize],
    /// 音轨 RGB 颜色数组，**按文档音轨索引**取值（`track_colors[doc_track]`）。
    ///
    /// 与音符层的取色规则同源（`onion_skin::stream` 的
    /// `current_track_color_f32(doc_track)`）：长度必须覆盖 `track_order` 中出现的
    /// 全部文档轨 id，不能退化成「定长镜像 + 取模」——调色板长度可变时会串色。
    pub track_colors: &'a [[f32; 3]],
    /// 音轨可见性标志位数组，`false` 表示该轨道本次不渲染
    pub track_visible: &'a [bool],
    /// MIDI 文档（音符权威数据源）；为 `None` 时不渲染任何音符
    pub midi_doc: Option<&'a lumino_midi_loader::MidiDocument>,
    /// 播放位置（tick），用于绘制演奏指示线；`<= 0.0` 时不绘制
    pub playback_position: f32,
    /// 走带视图颜色配置
    pub colors: &'a ArrangementViewColors,
    /// 音符条高度（像素）—— 必须与音符层 uniform 的 `note_height` 同值，
    /// 调用方取 [`crate::arrangement_renderer::ARRANGEMENT_NOTE_HEIGHT`]。
    pub note_height: f32,
    /// ghost 音符预览 `(tick_start, tick_end, 视觉轨, key)`。
    ///
    /// `视觉轨` 是侧边栏顺序索引（与选区同源），取色时经 `track_order` 映射回文档轨；
    /// `key` 参与纵向定位，缺失会退回泳道中线的固定值。
    pub ghost_notes: &'a [(f64, f64, usize, u8)],
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

/// 构建覆盖层（背景 + lane + 网格线），绘制在音符之下
pub fn build_arrangement_overlay_back(
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

    // ── 1. 背景 ──
    out.push(ArrangementNoteInstance::background(
        cox, coy, w, h, colors.bg,
    ));

    if nt == 0 {
        return;
    }

    let (tf, tl) = visible_trk_range(viewport, h, nt);

    // ── 2. Lane 背景 ──
    for (ti, _tid) in params.track_order.iter().enumerate() {
        if ti < tf || ti >= tl {
            continue;
        }
        if !params.track_visible.get(ti).copied().unwrap_or(true) {
            continue;
        }
        let lane_y = trk_screen_y(viewport, ti) + coy;
        let c = if ti % 2 == 0 {
            colors.lane_even
        } else {
            colors.lane_odd
        };
        out.push(ArrangementNoteInstance::lane(cox, lane_y, w, lh, c));
    }

    // ── 3. 网格线（按拍号变化，与标尺/真实小节边界一致）──
    let ts = (viewport.scroll_x / ppu) as u32;
    let te = ((viewport.scroll_x + w) / ppu) as u32;
    for tick in crate::grid::measure_line_ticks(ts, te, viewport.ppq as u32, params.time_signatures)
    {
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
}

/// 构建覆盖层（框选矩形 + 拖拽框选 + ghost 音符 + 演奏指示线），绘制在音符之上
pub fn build_arrangement_overlay_front(
    out: &mut Vec<ArrangementNoteInstance>,
    params: &ArrangementSceneParams<'_>,
) {
    push_selection_rect(out, params, params.sel_rect);
    push_selection_rect(out, params, params.drag_sel_rect);
    push_ghost_notes(out, params);
    push_playhead(out, params);
}

/// 框选 / 拖拽框选矩形：两者几何相同，只有数据来源不同。
fn push_selection_rect(
    out: &mut Vec<ArrangementNoteInstance>,
    params: &ArrangementSceneParams<'_>,
    rect: Option<(f64, f64, usize, usize)>,
) {
    let Some((t_start, t_end, track_lo, track_hi)) = rect else {
        return;
    };
    let viewport = params.viewport;
    let lh = viewport.track_height * viewport.zoom_y;
    let ppu = viewport.zoom_x.max(0.001);
    let cox = viewport.canvas_offset[0];
    let coy = viewport.canvas_offset[1];
    let sx = cox + (t_start as f32) * ppu - viewport.scroll_x;
    let ex = cox + (t_end as f32) * ppu - viewport.scroll_x;
    let sy = track_lo as f32 * lh - viewport.scroll_y + coy;
    let ey = (track_hi as f32 + 1.0) * lh - viewport.scroll_y + coy;
    out.push(ArrangementNoteInstance::selection_rect(
        sx.min(ex),
        sy.min(ey),
        sx.max(ex) - sx.min(ex),
        sy.max(ey) - sy.min(ey),
        params.colors.sel_rect,
    ));
}

/// ghost 音符预览。
///
/// 外观与真实音符**逐项对齐**。公式唯一权威 = `shaders/arrangement_note.wgsl`
/// 的 vs_main：`key_h = lane_height/128`、
/// `note_y = lane_top + (127-key)*key_h + key_h*0.5`、
/// `half_h = max(note_height*0.5, 0.5)`、`sy = note_y - half_h`、`sh = half_h*2`。
/// 着色器一旦改动，本段必须同步——否则预览会再次漂移（旧实现正是把 Y 写死成
/// 泳道中线 `lane_y + lh*0.5 - 2.0`，多 pitch 选区全塌在一条线上）。
fn push_ghost_notes(out: &mut Vec<ArrangementNoteInstance>, params: &ArrangementSceneParams<'_>) {
    if params.ghost_notes.is_empty() {
        return;
    }
    let viewport = params.viewport;
    let lh = viewport.track_height * viewport.zoom_y;
    let ppu = viewport.zoom_x.max(0.001);
    let nt = params.track_order.len();
    let cox = viewport.canvas_offset[0];
    let coy = viewport.canvas_offset[1];
    let key_h = lh / 128.0;
    let half_h = (params.note_height * 0.5).max(0.5);
    for (start, end, lane, key) in params.ghost_notes {
        let lane_i = *lane;
        // 越界泳道：与音符层一致地丢弃（音符层的 lane_index 越界会被 GPU 裁剪）
        if lane_i >= nt {
            continue;
        }
        // 静音/隐藏轨不画：音符层在该轨被 `lane_index = f32::MAX` 哨兵剔除，
        // lane 背景循环同样跳过。若无此判断，ghost 会悬在没有任何内容的空泳道上。
        if !params.track_visible.get(lane_i).copied().unwrap_or(true) {
            continue;
        }
        // 取色：泳道是**视觉位置**，而调色板索引必须用**文档轨**——与音符层
        // `onion_skin::stream` 的 `current_track_color_f32(doc_track)` 同规则。
        // 侧栏排序后视觉位置 ≠ 文档索引，直接用视觉位置取色会串色
        // （与 `resolve_selection` 修过的「视觉/文档坐标混用」是同一类坑）。
        let Some(&doc_track) = params.track_order.get(lane_i) else {
            continue;
        };
        let Some(rgb) = params.track_colors.get(doc_track) else {
            continue;
        };
        // 真实音符通体按描边色渲染（推导见
        // `ARRANGEMENT_NOTE_BORDER_DARKEN_FACTOR`），故这里预乘同一系数；
        // 覆盖层的 ghost 实例自身不加深（border_width = 0），避免二次压暗。
        let color = [
            rgb[0] * ARRANGEMENT_NOTE_BORDER_DARKEN_FACTOR,
            rgb[1] * ARRANGEMENT_NOTE_BORDER_DARKEN_FACTOR,
            rgb[2] * ARRANGEMENT_NOTE_BORDER_DARKEN_FACTOR,
        ];
        let lane_y = trk_screen_y(viewport, lane_i) + coy;
        let sx = cox + (*start as f32) * ppu - viewport.scroll_x;
        let sw = ((*end - *start) as f32) * ppu;
        let note_y = lane_y + (127.0 - *key as f32) * key_h + key_h * 0.5;
        let sy = note_y - half_h;
        out.push(ArrangementNoteInstance::ghost_note(
            sx,
            sy,
            sw,
            half_h * 2.0,
            color,
        ));
    }
}

/// 演奏指示线：只在视口内绘制。
fn push_playhead(out: &mut Vec<ArrangementNoteInstance>, params: &ArrangementSceneParams<'_>) {
    if params.playback_position <= 0.0 {
        return;
    }
    let viewport = params.viewport;
    let w = viewport.canvas_size[0];
    let h = viewport.canvas_size[1];
    let cox = viewport.canvas_offset[0];
    let coy = viewport.canvas_offset[1];
    let cx = tick_to_x(viewport, params.playback_position as f64);
    if cx >= cox && cx <= cox + w {
        out.push(ArrangementNoteInstance::playhead(
            cx,
            coy,
            2.0,
            h,
            params.colors.playhead,
        ));
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

#[cfg(test)]
mod tests;
