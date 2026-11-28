//! 卷帘骨架模型：视口内边距、每小节 tick 与滚动条比例。
//!
//! 键盘列与滚动条只由 egui 绘制；本片只把编辑器视图换算成 [`PianoRollView`]。

use lumino_ui_egui::chrome::piano_roll::PianoRollView;
use lumino_ui_egui::layout;

use crate::root::Root;

/// 滚动条比例计算所需的中间量。
struct ScrollMetrics {
    scrollable_x: f32,
    scrollable_y: f32,
    max_x: f32,
    max_y: f32,
    viewport_y: f32,
    thumb_x_ratio: f32,
    thumb_y_ratio: f32,
}

impl Root {
    /// 卷帘骨架模型。
    ///
    /// 只有「当前不是钢琴卷帘视图」与「画布尚未布局」两种情形返回 `None`。
    /// 洋葱皮 / 协作光标 / 循环区间 / 拍号分段都**不再**让键盘列与滚动条消失：
    /// 它们只影响叠加层；键盘列 / 滚动条 / 标尺全部由 egui 绘制
    /// （gfx 不补键盘，横向标尺也由 egui 按拍号分段画小节号）。
    pub(super) fn egui_piano_roll_model(&self) -> Option<PianoRollView> {
        let (left_inset, right_inset) = self.piano_roll_insets();
        if !self.right_sidebar_visible() {
            return None;
        }
        let view = &self.editor.editor_state.view;
        let canvas = self.editor.editor_state.canvas;
        if canvas.size_x <= 0.0 || canvas.size_y <= 0.0 {
            return None;
        }

        let vertical = self.sidebar.is_vertical_roll();
        let ppq = view.ppq;
        let ticks_per_measure = self.ticks_per_measure(ppq);
        let metrics = scroll_metrics(
            self.editor.editor_state.max_scroll,
            (canvas.size_x, canvas.size_y),
            view.keyboard_width,
            view.ruler_height,
            vertical,
        );

        let playhead_x = self
            .editor
            .get_playback_indicator_screen_x()
            .unwrap_or(view.keyboard_width);

        Some(PianoRollView {
            keyboard_width: view.keyboard_width,
            ruler_height: view.ruler_height,
            key_height: view.zoom_y,
            scroll_y: view.scroll_y,
            visible_keys: view.visible_key_count,
            zoom_x: view.zoom_x,
            scroll_x: view.scroll_x,
            ppq,
            ticks_per_measure,
            playhead_offset: playhead_x - view.keyboard_width,
            scroll_x_ratio: scroll_ratio(view.scroll_x, metrics.scrollable_x),
            scroll_y_ratio: scroll_ratio(view.scroll_y, metrics.scrollable_y),
            max_scroll_x: metrics.max_x,
            max_scroll_y: metrics.max_y,
            viewport_y: metrics.viewport_y,
            thumb_x_ratio: metrics.thumb_x_ratio,
            thumb_y_ratio: metrics.thumb_y_ratio,
            left_inset,
            // Windows/Linux 的两种标题栏模式在客户端顶部都有 30px 菜单栏行；
            // macOS 原生模式没有（主内容直接从工具栏开始）。
            top_inset: (if cfg!(target_os = "macos") && self.settings.synth.use_native_titlebar {
                0.0
            } else {
                layout::TITLEBAR_H
            }) + self.toolbar.height,
            right_inset,
            bottom_inset: layout::STATUSBAR_H
                + if self.sidebar.automation_panel_visible {
                    self.visual.velocity_panel_height
                } else {
                    0.0
                },
            vertical,
            time_signatures: self.editor.editor_state.data.time_signatures.clone(),
            // 演奏跟随：键盘列每帧读取播放键色（全零 = 无高亮 / 开关关闭）。
            key_colors: *self.editor.playback_key_colors(),
        })
    }

    /// 左右内边距：左栏固定 48 + 面板宽；右栏同理，未显示时为 0。
    fn piano_roll_insets(&self) -> (f32, f32) {
        let left_inset = 48.0
            + if self.sidebar.panel_visible {
                self.sidebar.panel_width
            } else {
                0.0
            };
        let right_inset = if self.right_sidebar_visible() {
            48.0 + if self.right_sidebar.panel_visible {
                self.right_sidebar.panel_width
            } else {
                0.0
            }
        } else {
            0.0
        };
        (left_inset, right_inset)
    }

    /// 拍号 → 每小节 tick（默认 4/4）。横向标尺已改为按段计算
    /// （`PianoRollView::time_signatures` → `ruler::measure_labels`），
    /// 这里只服务纵向卷帘的单段近似；同 tick 重复取最后一条（与 gfx 一致）。
    fn ticks_per_measure(&self, ppq: u16) -> f32 {
        let (numerator, denominator) = self
            .editor
            .editor_state
            .data
            .time_signatures
            .iter()
            .take_while(|&&(tick, _, _)| tick == 0)
            .last()
            .map(|&(_, numerator, denominator)| (numerator, denominator))
            .unwrap_or((4, 4));
        f32::from(ppq) * 4.0 * f32::from(numerator) / f32::from(denominator.max(1))
    }
}

/// 滚动条按 上一代 `ScrollbarWidget::thumb_geometry` 的口径换算成比例：
/// 水平 viewport 缺省为轨道长度；垂直 viewport 为画布高 - 标尺高。
/// 纵向卷帘转置：时间轴视口 = 画布高 - 键盘厚，音高轴视口 = 画布宽。
fn scroll_metrics(
    max: (f32, f32),
    canvas: (f32, f32),
    keyboard_width: f32,
    ruler_height: f32,
    vertical: bool,
) -> ScrollMetrics {
    let (max_x, max_y) = max;
    let (size_x, size_y) = canvas;
    let grid_h = (size_y - keyboard_width).max(1.0);
    let (track_x, track_y, viewport_x, viewport_y) = if vertical {
        (
            (size_y + 16.0).max(1.0),
            (size_x - 4.0).max(1.0),
            grid_h,
            size_x,
        )
    } else {
        (
            (size_x + 16.0).max(1.0),
            (size_y - 4.0).max(1.0),
            (size_x + 16.0).max(1.0),
            (size_y - ruler_height).max(0.0),
        )
    };
    let scrollable_x = (max_x - viewport_x).max(0.0);
    let scrollable_y = (max_y - viewport_y).max(0.0);
    let thumb = |track: f32, viewport: f32, max: f32| {
        if max <= viewport {
            1.0
        } else {
            (track * viewport / max).max(20.0).min(track) / track
        }
    };

    ScrollMetrics {
        scrollable_x,
        scrollable_y,
        max_x,
        max_y,
        viewport_y,
        thumb_x_ratio: thumb(track_x, viewport_x, max_x),
        thumb_y_ratio: thumb(track_y, viewport_y, max_y),
    }
}

fn scroll_ratio(scroll: f32, scrollable: f32) -> f32 {
    if scrollable <= 0.0 {
        0.0
    } else {
        scroll.clamp(0.0, scrollable) / scrollable
    }
}
