//! 渲染参数构建 — build_render_params

use crate::RenderParams;
use crate::host::Host;
use crate::host::render::data::{GridColors, RenderData};
use crate::titlebar::mode_toggle::AppMode;
use lumino_gfx::ArrangementUniform;

/// VS cull 直绘的可见占比阈值（PREF-005）。
///
/// 直绘占优条件为 `V > 0.85N`（见 `RenderParams::vs_cull_mode` 的实测表），
/// 这里取 0.9 留余量，避免在临界缩放来回切换。
const VS_CULL_VISIBLE_FRACTION_THRESHOLD: f32 = 0.9;

/// 估算「可见音符占比」（0~1）= 可见 tick 比例 × 可见 key 比例。
///
/// 用于 PREF-005 的 VS cull 直绘闸门。闸门判错会直接造成放大档 **8 倍回退**
/// （实测：1600 万音符里仅 1.5 万可见时，直绘 10.89ms vs cull 1.35ms），
/// 因此判据抽成纯函数并单测（`tests::estimate_visible_fraction_*`）。
///
/// 退化输入一律返回 0（保守关闸）：
/// - `zoom_x <= 0`（未初始化）：无法估算 → 不启用；
/// - `total_ticks == 0`（空工程）：无内容可见 → 不启用；
/// - 非有限结果（NaN/INF）：不启用。
fn estimate_visible_fraction(
    zoom_x: f32,
    canvas_width: f32,
    total_ticks: u32,
    visible_key_count: usize,
    max_key_index: f32,
) -> f32 {
    if zoom_x <= 0.0 || canvas_width <= 0.0 || total_ticks == 0 {
        return 0.0;
    }
    let viewport_ticks = (canvas_width / zoom_x).max(0.0);
    let tick_fraction = (viewport_ticks / total_ticks as f32).min(1.0);
    let key_axis = (max_key_index + 1.0).max(1.0);
    let key_fraction = (visible_key_count as f32 / key_axis).min(1.0);
    let fraction = tick_fraction * key_fraction;
    if fraction.is_finite() {
        fraction.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

impl Host {
    /// 构建渲染参数
    pub(crate) fn build_render_params(&mut self, data: RenderData) -> RenderParams {
        use crate::editor::velocity::PANEL_PADDING_Y;
        let es = &self.root.editor.editor_state;
        let physical_size = self.render_ctx.viewport.physical_size();
        let theme = self.root.theme();
        let colors = GridColors::from_theme(&theme);

        let bg_color = [
            colors.bg[0] as f64,
            colors.bg[1] as f64,
            colors.bg[2] as f64,
            colors.bg[3] as f64,
        ];
        let ppq = es.view.ppq;
        let is_arrangement_mode = self.root.is_arrangement_mode();
        let max_key_index = if is_arrangement_mode {
            let av = &self.root.arrangement_view.viewport;
            let track_count = self.root.sidebar.tracks.len().max(1) as f32;
            track_count * av.track_height - av.track_height / 128.0
        } else {
            (es.view.visible_key_count.saturating_sub(1)) as f32
        };

        // 从 collect_viewport_info 获取 canvas_offset 和 canvas_size
        let viewport_info = self.collect_viewport_info();
        let (canvas_offset, canvas_size, keyboard_width, ruler_height) = if is_arrangement_mode {
            (
                (viewport_info.canvas_offset.x, viewport_info.canvas_offset.y),
                (viewport_info.canvas_size.x, viewport_info.canvas_size.y),
                0.0,
                0.0,
            )
        } else {
            (
                (es.canvas.offset_x, es.canvas.offset_y),
                (es.canvas.size_x, es.canvas.size_y),
                es.view.keyboard_width,
                es.view.ruler_height,
            )
        };

        // 构建 arrangement uniform
        let bg_color_arr = colors.bg;
        let bar_color = colors.bar_line;
        let arrangement_uniform = if is_arrangement_mode {
            let av = &self.root.arrangement_view.viewport;
            let track_count = self.root.sidebar.tracks.len().max(1) as f32;
            let mut track_colors = [[0.0_f32; 4]; 16];
            // 使用当前调色板的颜色（来自 PaletteManager），
            // 超出调色板颜色数的轨道循环取色
            for (i, slot) in track_colors.iter_mut().enumerate() {
                *slot = lumino_extras::palette::current_track_color_f32(i);
            }
            let playhead_x = if self.root.editor.playback_position > 0.0 {
                self.root.editor.playback_position * av.zoom_x - data.scroll.0
            } else {
                -1.0
            };
            ArrangementUniform {
                scroll: [data.scroll.0, data.scroll.1],
                zoom: data.zoom.0,
                track_height: av.track_height,
                // 垂直缩放必须透传：音符 GPU 裁剪/绘制管线用 track_height*zoom_y
                // 计算泳道高度，与 CPU 端 lane 背景（arrangement_instances.rs 用同一
                // viewport.zoom_y）保持一致，否则缩放时音符泳道高度与背景失配。
                zoom_y: av.zoom_y,
                viewport_size: [data.viewport_size.width, data.viewport_size.height],
                canvas_offset: [canvas_offset.0, canvas_offset.1],
                playhead_x,
                bg_color: [
                    bg_color_arr[0],
                    bg_color_arr[1],
                    bg_color_arr[2],
                    bg_color_arr[3],
                ],
                bar_color: [bar_color[0], bar_color[1], bar_color[2], bar_color[3]],
                playhead_color: [
                    lumino_gfx::colors::AR_PLAYHEAD_COLOR.0,
                    lumino_gfx::colors::AR_PLAYHEAD_COLOR.1,
                    lumino_gfx::colors::AR_PLAYHEAD_COLOR.2,
                    lumino_gfx::colors::AR_PLAYHEAD_COLOR.3,
                ],
                track_colors,
                track_count,
                ..Default::default()
            }
        } else {
            ArrangementUniform::default()
        };

        // 计算力度面板矩形（用于 wgpu scissor 裁剪）
        // 仅在自动化面板可见时设置，否则跳过 CC bar 渲染器 prepare/draw
        let velocity_panel_rect =
            if is_arrangement_mode || !self.root.sidebar.automation_panel_visible {
                None
            } else {
                let es = &self.root.editor.editor_state;
                // velocity 面板在 grid Canvas 下方，间隔 0px
                const H_SCROLLBAR_HEIGHT: f32 = 20.0;
                Some((
                    es.canvas.offset_x,
                    es.canvas.offset_y + es.canvas.size_y + H_SCROLLBAR_HEIGHT,
                    es.canvas.size_x,
                    self.root.visual.velocity_panel_height + PANEL_PADDING_Y + 10.0,
                ))
            };

        // ── 内容脏标记（2026-10-01 滚动拖拽全程卡顿修复）──
        //
        // 决定调用方 present 前是否必须等待渲染线程提交本帧：
        // ① 生产者置位：音符编辑增量 / 洋葱皮重传 / 预览实例变化（新产生的数据）；
        // ② 首次提交或视口物理尺寸变化：渲染线程会重建离屏纹理，跳过等待可能拷到
        //    刚创建、尚未渲染的空纹理。
        //
        // 注意：随视口每帧重算的派生实例（网格/标尺/走带覆盖层/CC 柱）**不**置位——
        // 它们落后一帧无感知，置位会让纯滚动帧永远走等待路径、免等待策略失效。
        let viewport_key = (physical_size.width, physical_size.height);
        let viewport_changed = self.render_ctx.last_sent_viewport != Some(viewport_key);
        let content_dirty =
            std::mem::take(&mut self.render_ctx.render_content_dirty) || viewport_changed;
        self.render_ctx.last_sent_viewport = Some(viewport_key);

        // ── 亚像素档位（PREF-004 P1）──
        //
        // 判据：最细可画音符（吸附精度，单位 tick）在时间轴缩放下也不足 1 像素，
        // 即 `zoom_x × snap_precision < 1`。此时：
        //   - quad 的形状/描边已无视觉意义（真机上 quad 路径会因 horiz_margin 巨大
        //     把整块判成描边色，反而更失真）；
        //   - 每音符仍要付「4 顶点 + 2 三角形」的图元固定成本，真机实测该成本
        //     是全景帧的主导项（16M 全景 draw 11.97ms vs cull 1.39ms）。
        // 因此切点图元直绘。仅横向卷帘生效（纵向转置版无点入口）。
        let subpixel_note_mode = !es.is_vertical_roll
            && es.view.zoom_x > 0.0
            && es.view.snap_precision > 0.0
            && es.view.zoom_x * es.view.snap_precision < 1.0;

        // ── VS cull 直绘闸门（PREF-005）──
        //
        // 直绘把「一趟 compute 扫全量」换成「顶点着色器为全部实例各跑一遍」，
        // 真机实测（RTX 2060）：全量可见时直绘快 ~12%，而放大档（1.5 万可见 /
        // 1600 万总数）直绘慢 8 倍。由 `0.65N < 0.10N + 0.65V` 得占优条件
        // `V > 0.85N`，故此处按「可见 tick 比例 × 可见 key 比例」估算可见占比，
        // 超过阈值才启用（留余量取 0.9，避免临界抖动）。
        //
        // 两条路径已由 `direct_tests` 证明逐位像素等价 ⇒ 闸门切换零视觉风险。
        let visible_fraction = estimate_visible_fraction(
            es.view.zoom_x,
            canvas_size.0,
            es.view.total_ticks,
            es.view.visible_key_count as usize,
            max_key_index,
        );
        let vs_cull_mode =
            !es.is_vertical_roll && visible_fraction > VS_CULL_VISIBLE_FRACTION_THRESHOLD;

        RenderParams::builder()
            .viewport_size((physical_size.width, physical_size.height))
            .logical_size((data.viewport_size.width, data.viewport_size.height))
            .scale_factor(self.render_ctx.viewport.scale_factor())
            .scroll(data.scroll)
            .zoom(data.zoom)
            .keyboard_width(keyboard_width)
            .ruler_height(ruler_height)
            .canvas_offset((canvas_offset.0, canvas_offset.1))
            .canvas_size((canvas_size.0, canvas_size.1))
            .background_color(bg_color)
            .color_bg(colors.bg)
            .color_bg_black_key(colors.black_key)
            .color_bar(colors.bar_line)
            .color_beat(colors.beat_line)
            .color_half_beat(colors.half_beat_line)
            .color_grid(colors.grid_line)
            .color_key_line(colors.key_line)
            .ppq(ppq as f32)
            .max_key_index(max_key_index)
            .is_arrangement_mode(is_arrangement_mode)
            .ruler_instances(data.ruler_instances)
            .time_signatures(es.data.time_signatures.clone())
            .arrangement_overlay_instances(data.arrangement_overlay_instances)
            .arrangement_overlay_back_len(data.arrangement_overlay_back_len)
            .arrangement_track_order(
                data.arrangement_track_order
                    .iter()
                    .map(|&t| t as u32)
                    .collect(),
            )
            .arrangement_track_visible(data.arrangement_track_visible)
            .arrangement_uniform(arrangement_uniform)
            .cc_bar_instances(data.cc_bar_instances)
            .velocity_panel_rect(velocity_panel_rect)
            .skip_scene_render(self.root.state.current_mode == AppMode::Waterfall)
            .is_vertical_roll(self.root.editor.editor_state.is_vertical_roll)
            .content_dirty(content_dirty)
            .subpixel_note_mode(subpixel_note_mode)
            .vs_cull_mode(vs_cull_mode)
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::{VS_CULL_VISIBLE_FRACTION_THRESHOLD, estimate_visible_fraction};

    /// 1920px 画布 + 128 键（max_key_index = 127）
    const CANVAS_W: f32 = 1920.0;
    const KEYS: usize = 128;
    const MAX_KEY: f32 = 127.0;

    /// 闸门必须**打开**：全曲缩放到屏 + 全键可见（可见占比 ≈ 1.0）。
    ///
    /// 这是直绘唯一占优的档位（实测省掉 cull pass ≈ 10%）。
    #[test]
    fn estimate_visible_fraction_full_view_opens_gate() {
        // 全曲 4M tick，zoom_x 取「整曲刚好入屏」
        let zoom_x = CANVAS_W / 4_000_000.0;
        let fraction = estimate_visible_fraction(zoom_x, CANVAS_W, 4_000_000, KEYS, MAX_KEY);
        assert!(
            fraction > VS_CULL_VISIBLE_FRACTION_THRESHOLD,
            "全曲视图应开闸，实际占比 {fraction}"
        );
    }

    /// 闸门必须**关闭**：放大档（可见 tick 占比极小）。
    ///
    /// 这是本卡最关键的回归守卫：该档实测直绘比 cull 慢 8 倍
    /// （1600 万音符 / 1.5 万可见：10.89ms vs 1.35ms）。
    #[test]
    fn estimate_visible_fraction_zoomed_in_keeps_gate_closed() {
        // 2 px/tick × 52 键可见
        let fraction = estimate_visible_fraction(2.0, CANVAS_W, 4_000_000, 52, MAX_KEY);
        assert!(
            fraction <= VS_CULL_VISIBLE_FRACTION_THRESHOLD,
            "放大档必须关闸（否则 8 倍回退），实际占比 {fraction}"
        );
        assert!(
            fraction < 0.01,
            "放大档可见占比应远低于阈值，实际 {fraction}"
        );
    }

    /// 退化输入一律保守关闸，不得产生 NaN/INF 让比较式误判为「开」。
    #[test]
    fn estimate_visible_fraction_degenerate_inputs_keep_gate_closed() {
        for (zoom_x, canvas_w, total_ticks, label) in [
            (0.0, CANVAS_W, 4_000_000, "zoom_x = 0"),
            (-1.0, CANVAS_W, 4_000_000, "zoom_x < 0"),
            (2.0, 0.0, 4_000_000, "画布宽 0"),
            (2.0, CANVAS_W, 0, "空工程 total_ticks = 0"),
            (f32::NAN, CANVAS_W, 4_000_000, "zoom_x = NaN"),
            (f32::INFINITY, CANVAS_W, 4_000_000, "zoom_x = INF"),
        ] {
            let fraction = estimate_visible_fraction(zoom_x, canvas_w, total_ticks, KEYS, MAX_KEY);
            assert!(
                fraction.is_finite() && fraction <= VS_CULL_VISIBLE_FRACTION_THRESHOLD,
                "{label}：应保守关闸，实际 {fraction}"
            );
        }
    }
}
