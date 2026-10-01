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
    let viewport = params.viewport;
    let colors = params.colors;
    let w = viewport.canvas_size[0];
    let h = viewport.canvas_size[1];
    let lh = viewport.track_height * viewport.zoom_y;
    let ppu = viewport.zoom_x.max(0.001);
    let nt = params.track_order.len();
    let cox = viewport.canvas_offset[0];
    let coy = viewport.canvas_offset[1];

    // ── 1. 框选矩形 ──
    if let Some((t_start, t_end, track_lo, track_hi)) = params.sel_rect {
        let sx = cox + (t_start as f32) * ppu - viewport.scroll_x;
        let ex = cox + (t_end as f32) * ppu - viewport.scroll_x;
        let sy = track_lo as f32 * lh - viewport.scroll_y + coy;
        let ey = (track_hi as f32 + 1.0) * lh - viewport.scroll_y + coy;
        out.push(ArrangementNoteInstance::selection_rect(
            sx.min(ex),
            sy.min(ey),
            sx.max(ex) - sx.min(ex),
            sy.max(ey) - sy.min(ey),
            colors.sel_rect,
        ));
    }

    // ── 2. 拖拽框选矩形 ──
    if let Some((t_start, t_end, track_lo, track_hi)) = params.drag_sel_rect {
        let sx = cox + (t_start as f32) * ppu - viewport.scroll_x;
        let ex = cox + (t_end as f32) * ppu - viewport.scroll_x;
        let sy = track_lo as f32 * lh - viewport.scroll_y + coy;
        let ey = (track_hi as f32 + 1.0) * lh - viewport.scroll_y + coy;
        out.push(ArrangementNoteInstance::selection_rect(
            sx.min(ex),
            sy.min(ey),
            sx.max(ex) - sx.min(ex),
            sy.max(ey) - sy.min(ey),
            colors.sel_rect,
        ));
    }

    // ── 3. ghost 音符预览 ──
    //
    // 外观与真实音符**逐项对齐**。公式唯一权威 = `shaders/arrangement_note.wgsl`
    // 的 vs_main：`key_h = lane_height/128`、
    // `note_y = lane_top + (127-key)*key_h + key_h*0.5`、
    // `half_h = max(note_height*0.5, 0.5)`、`sy = note_y - half_h`、`sh = half_h*2`。
    // 着色器一旦改动，本段必须同步——否则预览会再次漂移（旧实现正是把 Y 写死成
    // 泳道中线 `lane_y + lh*0.5 - 2.0`，多 pitch 选区全塌在一条线上）。
    if !params.ghost_notes.is_empty() {
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
mod tests {
    use super::*;
    use crate::arrangement_renderer::ARRANGEMENT_NOTE_HEIGHT;

    /// 测试视口：单泳道高 48px、无滚动、无缩放、画布偏移为 0。
    fn viewport() -> ArrangementViewport {
        ArrangementViewport {
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom_x: 200.0,
            zoom_y: 1.0,
            track_height: 48.0,
            canvas_offset: [0.0, 0.0],
            canvas_size: [800.0, 600.0],
            total_ticks: 7680,
            ppq: 480,
        }
    }

    fn colors() -> ArrangementViewColors {
        ArrangementViewColors {
            bg: [0.0, 0.0, 0.0],
            lane_even: [0.0, 0.0, 0.0],
            lane_odd: [0.0, 0.0, 0.0],
            measure_line: [0.0, 0.0, 0.0, 1.0],
            playhead: [0.0, 0.0, 0.0, 1.0],
            sel_rect: [0.0, 0.0, 0.0],
        }
    }

    /// 构建前景覆盖层并只保留 ghost 实例（tag 5）。
    ///
    /// `playback_position = 0.0`、`sel_rect` / `drag_sel_rect` 均为 `None`，
    /// 因此产出物里除 ghost 外不会有其它实例。
    #[allow(clippy::too_many_arguments)]
    fn build_ghosts(
        viewport: &ArrangementViewport,
        colors: &ArrangementViewColors,
        track_order: &[usize],
        track_colors: &[[f32; 3]],
        track_visible: &[bool],
        ghost_notes: &[(f64, f64, usize, u8)],
        note_height: f32,
    ) -> Vec<ArrangementNoteInstance> {
        let params = ArrangementSceneParams {
            viewport,
            track_order,
            track_colors,
            track_visible,
            midi_doc: None,
            playback_position: 0.0,
            colors,
            note_height,
            ghost_notes,
            sel_rect: None,
            drag_sel_rect: None,
            time_signatures: &[],
        };
        let mut out = Vec::new();
        build_arrangement_overlay_front(&mut out, &params);
        out.retain(|i| i.tag == 5);
        out
    }

    /// 解包 `rgba_packed` → `(r8, g8, b8, a8)`。
    fn unpack_rgba(instance: &ArrangementNoteInstance) -> (u8, u8, u8, u8) {
        (
            (instance.rgba_packed & 0xFF) as u8,
            ((instance.rgba_packed >> 8) & 0xFF) as u8,
            ((instance.rgba_packed >> 16) & 0xFF) as u8,
            ((instance.rgba_packed >> 24) & 0xFF) as u8,
        )
    }

    /// ghost 的 Y/高度必须逐字复刻 `shaders/arrangement_note.wgsl`：
    /// `key_h = lane_height/128`、`note_y = lane_top + (127-key)*key_h + key_h*0.5`、
    /// `sy = note_y - half_h`、`sh = 2*half_h`。
    ///
    /// 旧实现写死 `lane_top + lh*0.5 - 2.0`（本例 = 22.0），本用例就是防它回归。
    #[test]
    fn test_ghost_y_and_height_follow_real_note_formula() {
        let vp = viewport();
        let colors = colors();
        let order = [0usize];
        let palette = [[0.0f32, 0.0, 0.0]];
        let visible = [true];
        let ghosts = [(0.0, 480.0, 0, 60u8)];

        let out = build_ghosts(
            &vp,
            &colors,
            &order,
            &palette,
            &visible,
            &ghosts,
            ARRANGEMENT_NOTE_HEIGHT,
        );

        assert_eq!(out.len(), 1, "应产出 1 个 ghost 实例");
        let ghost = &out[0];
        let lane_y = 0.0_f32;
        let key_h = vp.track_height * vp.zoom_y / 128.0;
        let expected_y =
            lane_y + (127.0 - 60.0) * key_h + key_h * 0.5 - ARRANGEMENT_NOTE_HEIGHT * 0.5;

        assert!(
            (ghost.y - expected_y).abs() < 1e-4,
            "ghost Y 应为 {expected_y}（真实音符公式），实际 {}",
            ghost.y
        );
        assert!(
            (ghost.h - ARRANGEMENT_NOTE_HEIGHT).abs() < 1e-4,
            "ghost 高度应等于音符条高度 {}，实际 {}",
            ARRANGEMENT_NOTE_HEIGHT,
            ghost.h
        );
        assert!(
            (ghost.y - (lane_y + vp.track_height * vp.zoom_y * 0.5 - 2.0)).abs() > 1.0,
            "不得回归到泳道中线死值 lane_y + lh*0.5 - 2.0"
        );
    }

    /// 多 pitch 选区不得塌到同一条线上：相邻半音差 = key_h，本例 12 个半音。
    #[test]
    fn test_ghost_multi_pitch_spans_lane_vertically() {
        let vp = viewport();
        let colors = colors();
        let order = [0usize];
        let palette = [[0.0f32, 0.0, 0.0]];
        let visible = [true];
        let ghosts = [(0.0, 480.0, 0, 60u8), (0.0, 480.0, 0, 72u8)];

        let out = build_ghosts(
            &vp,
            &colors,
            &order,
            &palette,
            &visible,
            &ghosts,
            ARRANGEMENT_NOTE_HEIGHT,
        );

        assert_eq!(out.len(), 2);
        let key_h = vp.track_height * vp.zoom_y / 128.0;
        // 高音在上：key 72 的 Y 比 key 60 小 12*key_h
        let dy = out[0].y - out[1].y;
        assert!(
            (dy - 12.0 * key_h).abs() < 1e-4,
            "高 12 个半音应上移 12*key_h = {}，实际 {dy}",
            12.0 * key_h
        );
    }

    /// 高度必须取自 `note_height` 参数（不得硬编码 4.0）：换 8.0 时 Y/高度同步变。
    #[test]
    fn test_ghost_height_follows_note_height_param() {
        let vp = viewport();
        let colors = colors();
        let order = [0usize];
        let palette = [[0.0f32, 0.0, 0.0]];
        let visible = [true];
        let ghosts = [(0.0, 480.0, 0, 60u8)];

        let out = build_ghosts(&vp, &colors, &order, &palette, &visible, &ghosts, 8.0);

        assert!((out[0].h - 8.0).abs() < 1e-4, "高度应跟随 note_height 参数");
        let key_h = vp.track_height * vp.zoom_y / 128.0;
        let expected_y = (127.0 - 60.0) * key_h + key_h * 0.5 - 4.0;
        assert!(
            (out[0].y - expected_y).abs() < 1e-4,
            "half_h 随高度变化，Y 必须同步，期望 {expected_y}，实际 {}",
            out[0].y
        );
    }

    /// 取色与真实音符同源：调色板色 × `BORDER_DARKEN_FACTOR`、alpha 1.0、无圆角无边框。
    ///
    /// 真实音符在走带里通体按描边色渲染（4px 高 + 1px 边框 → hy = 0.5），
    /// 故白 1.0 的轨道色最终落在 1.0 × 0.4 × 255 + 0.5 = 102。
    /// 若与 shader 同步调整了加深系数，本断言需一并更新。
    #[test]
    fn test_ghost_color_matches_real_note_rendering() {
        let vp = viewport();
        let colors = colors();
        let order = [0usize];
        let palette = [[1.0f32, 0.5, 0.25]];
        let visible = [true];
        let ghosts = [(0.0, 480.0, 0, 60u8)];

        let out = build_ghosts(
            &vp,
            &colors,
            &order,
            &palette,
            &visible,
            &ghosts,
            ARRANGEMENT_NOTE_HEIGHT,
        );

        assert_eq!(
            unpack_rgba(&out[0]),
            (102, 51, 26, 255),
            "色/alpha 必须与真实音符一致（调色板色 × 0.4、a = 1.0）"
        );
        assert_eq!(
            out[0].props_packed, 0,
            "不得带圆角/边框（真实音符是硬边矩形，且覆盖层边框会造成二次压暗）"
        );
    }

    /// 侧栏排序后 `track_order` 非恒等：ghost 在**视觉轨 0**，颜色必须取
    /// **文档轨 1** 的调色板色（音符层按文档轨取色）。按视觉轨取色会串色。
    #[test]
    fn test_ghost_color_uses_document_track_not_visual_lane() {
        let vp = viewport();
        let colors = colors();
        let order = [1usize, 0];
        let palette = [[0.1f32, 0.1, 0.1], [1.0, 1.0, 1.0]];
        let visible = [true, true];
        let ghosts = [(0.0, 480.0, 0, 60u8)];

        let out = build_ghosts(
            &vp,
            &colors,
            &order,
            &palette,
            &visible,
            &ghosts,
            ARRANGEMENT_NOTE_HEIGHT,
        );

        assert_eq!(out.len(), 1);
        assert_eq!(
            unpack_rgba(&out[0]),
            (102, 102, 102, 255),
            "视觉轨 0 对应文档轨 1（白色），取色必须经 track_order 映射"
        );
        // 泳道定位仍按视觉轨 0：不含泳道偏移
        let key_h = vp.track_height * vp.zoom_y / 128.0;
        let expected_y = (127.0 - 60.0) * key_h + key_h * 0.5 - ARRANGEMENT_NOTE_HEIGHT * 0.5;
        assert!((out[0].y - expected_y).abs() < 1e-4, "泳道须按视觉位置定位");
    }

    /// 静音/隐藏轨不画 ghost：音符层在该轨被 `lane_index = f32::MAX` 哨兵剔除，
    /// lane 背景循环同样跳过——否则 ghost 会悬在没有内容的空泳道上。
    #[test]
    fn test_ghost_skips_muted_lane() {
        let vp = viewport();
        let colors = colors();
        let order = [0usize, 1];
        let palette = [[1.0f32, 1.0, 1.0], [1.0, 1.0, 1.0]];
        let visible = [true, false];
        let ghosts = [(0.0, 480.0, 1, 60u8)];

        let out = build_ghosts(
            &vp,
            &colors,
            &order,
            &palette,
            &visible,
            &ghosts,
            ARRANGEMENT_NOTE_HEIGHT,
        );

        assert!(out.is_empty(), "静音轨上不得绘制 ghost");
    }

    /// 越界泳道 / 取色表越界一律安全丢弃（不得 panic，也不得画出幽灵泳道）。
    #[test]
    fn test_ghost_out_of_range_lane_is_dropped() {
        let vp = viewport();
        let colors = colors();
        let order = [0usize];
        let palette = [[1.0f32, 1.0, 1.0]];
        let visible = [true];

        let lane_beyond_track_order = [(0.0, 480.0, 7, 60u8)];
        assert!(
            build_ghosts(
                &vp,
                &colors,
                &order,
                &palette,
                &visible,
                &lane_beyond_track_order,
                ARRANGEMENT_NOTE_HEIGHT,
            )
            .is_empty(),
            "视觉轨越界应丢弃"
        );

        let doc_track_beyond_palette = [(0.0, 480.0, 0, 60u8)];
        let empty_palette: [[f32; 3]; 0] = [];
        assert!(
            build_ghosts(
                &vp,
                &colors,
                &order,
                &empty_palette,
                &visible,
                &doc_track_beyond_palette,
                ARRANGEMENT_NOTE_HEIGHT,
            )
            .is_empty(),
            "取色表缺该文档轨时应丢弃，不得越界"
        );
    }

    /// 最小宽度与真实音符一致：`arrangement_note.wgsl` 用 `max(length*ppu, 1.0)`，
    /// 覆盖层不得再夹到 2.0。
    #[test]
    fn test_ghost_min_width_matches_real_note() {
        let mut vp = viewport();
        vp.zoom_x = 1.0; // 0.5 tick → 0.5px，触发最小宽度夹取
        let colors = colors();
        let order = [0usize];
        let palette = [[1.0f32, 1.0, 1.0]];
        let visible = [true];
        let ghosts = [(10.0, 10.5, 0, 60u8)];

        let out = build_ghosts(
            &vp,
            &colors,
            &order,
            &palette,
            &visible,
            &ghosts,
            ARRANGEMENT_NOTE_HEIGHT,
        );

        assert_eq!(out.len(), 1);
        assert!(
            (out[0].w - 1.0).abs() < 1e-6,
            "最小宽度应为 1.0（与真实音符一致），实际 {}",
            out[0].w
        );
    }
}
