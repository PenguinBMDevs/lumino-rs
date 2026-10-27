//! 帧循环：全量帧、滚动增量帧、编辑拖动预览、呈现桥接。

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use winit::event_loop::ActiveEventLoop;

use super::{App, canvas::shift_canvas_pixels};
use crate::{
    camera::Camera,
    edit_overlay,
    render::{RenderStats, band::LodMode, density},
};

impl App {
    pub(super) fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let frame_start = Instant::now();
        let gap_ms = frame_start
            .duration_since(self.last_frame_start)
            .as_secs_f32()
            * 1000.0;
        self.last_frame_start = frame_start;
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        let (camera, bench_active, bench_finished) = {
            let Some(camera) = self.camera.as_mut() else {
                return;
            };
            let bench_active = if self.bench_ready {
                self.bench.as_mut().is_some_and(|bench| bench.apply(camera))
            } else {
                self.bench.is_some()
            };
            let bench_finished = self.bench.as_ref().is_some_and(|bench| bench.finished);
            if !bench_active && !bench_finished {
                camera.animate(dt);
            }
            (camera.clone(), bench_active, bench_finished)
        };

        if bench_finished && self.bench_ready {
            // 最后一帧不再做全量收尾（那会把 30ms 级收尾算进基准），
            // 直接按已记录的滚动条带帧出报告。
            self.finish_bench(event_loop);
            return;
        }

        let width = self.renderer.width();
        let height = self.renderer.height();
        let width_usize = width as usize;
        let height_usize = height as usize;
        if self.editor.drag.is_some() && !self.drag_base.is_empty() {
            self.redraw_drag_preview(&camera, width_usize, height_usize);
            self.next_frame = Instant::now();
            return;
        }
        let keyboard = (camera.keyboard_width.round().max(0.0) as usize).min(width_usize);
        let canvas_width = width_usize.saturating_sub(keyboard);
        let estimate = self.estimated_visible(&camera);
        let moving = bench_active || camera.is_moving();
        let dx = camera.scroll_x - self.last_scroll_x;
        let dy = camera.scroll_y - self.last_scroll_y;
        let zoom_same = (camera.zoom_x - self.last_zoom_x).abs() < 1e-6
            && (camera.zoom_y - self.last_zoom_y).abs() < 1e-6;
        let size_same = self.last_width == width && self.last_height == height;
        let can_scroll = self.frame_valid
            && !self.content_dirty
            && self.baseline_mode != LodMode::Overview
            && zoom_same
            && size_same
            && dy.abs() < 0.01
            && canvas_width > 0;

        // 亚像素滚动：屏幕内容还没变化，等积累到 1px 再平移。
        if moving && can_scroll && dx.abs() < 0.5 && !bench_finished {
            self.next_frame = Instant::now() + self.frame_interval;
            return;
        }

        // 滚动增量帧：平移已有帧，只重新光栅化露出的窄条；音符仍是完整矩形。
        let do_scroll =
            can_scroll && moving && dx.abs() >= 0.5 && dx.abs() <= canvas_width as f32 * 0.6;
        if do_scroll {
            let dx_int = dx.round() as i32;
            if dx_int == 0 {
                self.next_frame = Instant::now() + self.frame_interval;
                return;
            }
            self.shift_canvas(dx_int);
            // 帧的实际滚动 = 原帧滚动 + 整数平移。窄条必须用这个滚动渲染，
            // 不能用 camera.scroll_x：后者带亚像素残差，会让复用区和新条带
            // 逐渐错位（新进入视口的音符看起来"坏掉"）。
            self.last_scroll_x += dx_int as f32;
            let mut frame_camera = camera.clone();
            frame_camera.scroll_x = self.last_scroll_x;
            frame_camera.target_scroll_x = self.last_scroll_x;
            let (strip_x0, strip_x1) = if dx_int > 0 {
                let start = width_usize.saturating_sub(dx_int as usize);
                (start as u32, width)
            } else {
                let end = (keyboard + (-dx_int) as usize).min(width_usize);
                (keyboard as u32, end as u32)
            };
            // 滚动条带与铺底帧保持同一档位，避免复用区/新条带出现风格接缝。
            // 快速滚动/大跨度全量回退时铺底帧是 Motion（仍为矩形，仅省描边），
            // 条带也跟随 Motion；静止后再全量 Exact。
            let strip_mode = self.baseline_mode;
            let strip_stride = self.baseline_stride;
            let frame_estimate = self.estimated_visible(&frame_camera);
            let render_start = Instant::now();
            let band_stats = self.renderer.render_strip_into_frame(
                Arc::clone(&self.document),
                Arc::clone(&self.index),
                &frame_camera,
                self.current_track,
                strip_mode,
                strip_stride,
                strip_x0,
                strip_x1,
            );
            let render_done = Instant::now();
            let render_stats = RenderStats {
                visible_notes: frame_estimate,
                candidates: band_stats.candidates,
                long_candidates: band_stats.long_candidates,
                bands: 1,
                per_band: vec![band_stats],
            };
            let hud = self.hud_text(strip_mode.name(), frame_estimate);
            self.draw_overlays(&frame_camera, width_usize, height_usize, &hud);
            let overlay_done = Instant::now();
            self.present_canvas(&frame_camera);
            let present_done = Instant::now();
            self.last_scroll_y = camera.scroll_y;
            self.last_zoom_x = camera.zoom_x;
            self.last_zoom_y = camera.zoom_y;
            self.last_width = width;
            self.last_height = height;
            self.last_path = if strip_mode == LodMode::Exact {
                "scroll-exact"
            } else {
                "scroll-motion"
            };
            self.record_frame(
                render_start,
                render_done,
                overlay_done,
                present_done,
                &render_stats,
                self.last_path,
                dt,
                gap_ms,
            );
            if bench_finished {
                self.finish_bench(event_loop);
                return;
            }
            self.next_frame = Instant::now() + self.frame_interval;
            return;
        }

        // 全量帧：首帧 / 缩放 / 纵向滚动 / 编辑脏 / 大跨度跳跃。
        // 基准首帧强制 Exact 铺底，让后续条带复用全质量矩形；不计入统计。
        let force_motion = if self.bench_ready {
            bench_active || bench_finished
        } else {
            false
        };
        let mode = self.choose_mode(&camera, force_motion);
        let track_stride = self.choose_track_stride(&camera, force_motion);
        let render_start = Instant::now();
        let render_stats = if mode == LodMode::Overview {
            // 总览是密度热力图，半分辨率渲染后再最近邻放大：1.4M 像素/帧
            // 降到 0.35M，纯绘制可稳定低于 12.5ms；热力图本身是块状色块，
            // 2× 放大不损失可读性。
            let scale = 0.5_f32;
            let small_camera = camera.scaled(scale);
            let small_width = small_camera.width as usize;
            let small_height = small_camera.height as usize;
            if self.overview_buf.len() != small_width * small_height {
                self.overview_buf.resize(small_width * small_height, 0);
            }
            density::render_overview(
                &mut self.overview_buf,
                small_width,
                small_height,
                &small_camera,
                &self.density,
            );
            let mut x_map = vec![0usize; width_usize];
            for (x, slot) in x_map.iter_mut().enumerate() {
                *slot = (x * small_width / width_usize.max(1)).min(small_width - 1);
            }
            let mut y_map = vec![0usize; height_usize];
            for (y, slot) in y_map.iter_mut().enumerate() {
                *slot = (y * small_height / height_usize.max(1)).min(small_height - 1);
            }
            let frame = self.renderer.frame_mut();
            let top = (camera.ruler_height.round().max(0.0) as usize).min(height_usize);
            let left = (camera.keyboard_width.round().max(0.0) as usize).min(width_usize);
            for y in top..height_usize {
                let source_row =
                    &self.overview_buf[y_map[y] * small_width..(y_map[y] + 1) * small_width];
                let destination = &mut frame[y * width_usize..(y + 1) * width_usize];
                for x in left..width_usize {
                    destination[x] = source_row[x_map[x]];
                }
            }
            RenderStats {
                visible_notes: estimate,
                candidates: 0,
                long_candidates: 0,
                bands: 0,
                per_band: Vec::new(),
            }
        } else {
            self.renderer.render(
                Arc::clone(&self.document),
                Arc::clone(&self.index),
                &camera,
                self.current_track,
                mode,
                track_stride,
            )
        };
        let render_done = Instant::now();

        let hud = self.hud_text(mode.name(), estimate);
        if mode == LodMode::Overview && self.frame_valid {
            self.draw_overview_overlays(&camera, width_usize, height_usize, &hud);
        } else {
            self.draw_overlays(&camera, width_usize, height_usize, &hud);
        }
        self.draw_edit_selection(&camera, width_usize);
        let overlay_done = Instant::now();
        if mode == LodMode::Overview && self.frame_valid && !self.frame_index.is_multiple_of(30) {
            self.present_canvas(&camera);
        } else {
            self.present();
        }
        let present_done = Instant::now();

        self.last_scroll_x = camera.scroll_x;
        self.last_scroll_y = camera.scroll_y;
        self.last_zoom_x = camera.zoom_x;
        self.last_zoom_y = camera.zoom_y;
        self.last_width = width;
        self.last_height = height;
        self.last_path = mode.name();
        self.frame_valid = true;
        self.content_dirty = false;
        if mode != LodMode::Overview {
            self.baseline_mode = mode;
            self.baseline_stride = track_stride.max(1);
        } else {
            self.baseline_mode = LodMode::Overview;
        }

        if !self.bench_ready && self.bench.is_some() {
            // 基准首帧：完成一次全量（Motion 矩形）铺底，不计入统计。
            self.bench_ready = true;
            self.bench_warmup_until = Some(Instant::now() + Duration::from_secs(1));
            self.stats.reset();
            self.next_frame = Instant::now() + self.frame_interval;
            return;
        }

        self.record_frame(
            render_start,
            render_done,
            overlay_done,
            present_done,
            &render_stats,
            mode.name(),
            dt,
            gap_ms,
        );
        if bench_finished {
            self.finish_bench(event_loop);
            return;
        }
        if !moving && !self.idle_reported {
            self.stats.print_idle(self.index.total_notes);
            self.idle_reported = true;
        }
        self.next_frame = Instant::now() + self.frame_interval;
    }

    fn shift_canvas(&mut self, dx: i32) {
        let width = self.renderer.width() as usize;
        let height = self.renderer.height() as usize;
        if width == 0 || height == 0 {
            return;
        }
        let keyboard = self
            .camera
            .as_ref()
            .map_or(0, |camera| camera.keyboard_width.round().max(0.0) as usize)
            .min(width);
        let frame = self.renderer.frame_mut();
        if frame.len() != width * height {
            return;
        }
        shift_canvas_pixels(frame, width, height, keyboard, dx);
    }

    #[allow(clippy::too_many_arguments)]
    fn record_frame(
        &mut self,
        render_start: Instant,
        render_done: Instant,
        overlay_done: Instant,
        present_done: Instant,
        render_stats: &RenderStats,
        mode: &'static str,
        dt: f32,
        gap_ms: f32,
    ) {
        if self.frame_index < 12 && std::env::var_os("CPU_ONIONSKIN_DEBUG_FRAMES").is_some() {
            println!(
                "[frame {}] gap={:.1} render={:.2} overlay={:.2} present={:.2} ms visible={} cand={} long={} mode={} path={}",
                self.frame_index,
                gap_ms,
                (render_done - render_start).as_secs_f32() * 1000.0,
                (overlay_done - render_done).as_secs_f32() * 1000.0,
                (present_done - overlay_done).as_secs_f32() * 1000.0,
                render_stats.visible_notes,
                render_stats.candidates,
                render_stats.long_candidates,
                mode,
                self.last_path
            );
        }
        self.frame_index += 1;
        let draw_ms = (overlay_done - render_start).as_secs_f32() * 1000.0;
        let present_ms = (present_done - overlay_done).as_secs_f32() * 1000.0;
        let work_ms = (present_done - render_start).as_secs_f32() * 1000.0;
        if dt > 0.000_001 {
            let instant = 1.0 / dt;
            self.fps_ema = if self.fps_ema <= 0.0 {
                instant
            } else {
                self.fps_ema * 0.9 + instant * 0.1
            };
        }
        self.last_work_ms = work_ms;
        if let Some(until) = self.bench_warmup_until {
            if Instant::now() < until {
                return;
            }
            // 1 秒暖机：排除窗口首次合成/DWM 冷启动尖峰后再统计。
            self.bench_warmup_until = None;
            self.stats.reset();
        }
        self.stats.record_full(
            work_ms,
            draw_ms,
            present_ms,
            render_stats.visible_notes,
            mode,
            render_stats.bands,
        );
        // 基准模式只记录、不逐秒打印/改标题：同步 stdout 与 SetWindowText 都会
        // 在测量窗口里制造 10ms 级假尖峰。交互模式才输出实时行。
        if self.bench.is_none() || self.bench_reported {
            self.stats.maybe_report(self.window.as_deref());
        }
    }

    fn redraw_drag_preview(&mut self, camera: &Camera, width: usize, height: usize) {
        if self.drag_base.len() != width * height {
            self.drag_base.clear();
            self.content_dirty = true;
            return;
        }
        let style = Arc::clone(self.renderer.style());
        {
            let frame = self.renderer.frame_mut();
            frame.copy_from_slice(&self.drag_base);
            if let Some(drag) = self.editor.drag.as_ref() {
                edit_overlay::draw_drag(frame, width, camera, drag, &style, self.current_track);
            }
        }
        let hud = self.hud_text(self.editor.tool_name(), 0);
        self.draw_overlays(camera, width, height, &hud);
        self.present();
    }
}
