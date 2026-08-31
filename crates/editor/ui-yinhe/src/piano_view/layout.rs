//! 钢琴卷帘布局 — 复用 `lumino_core::ViewState` 的坐标变换
//!
//! 对应 `yinhe/crates/yinhe-egui/src/piano_view/layout.rs:131`。
//! yinhe `Layout` 同时计算 `content_rect / music_rect / keyboard_rect / ruler_rect` + `w/h/pw/ph`
//! 并在此 clamp `PianoRollView::scroll`。lumino 侧 ViewState 已有
//! `tick_to_x / x_to_tick / key_to_y / y_to_key / snap_tick` 等完整坐标系，
//! 本文件仅做 **布局几何计算 + ViewState 复用**，不自建 wgpu 资源。

use iced_core::{Point, Rectangle, Size};
use lumino_core::ViewState;

/// 钢琴卷帘方向（对应 yinhe `Orientation::Horizontal / Vertical`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default]
    Horizontal,
    Vertical,
}

/// 钢琴卷帘主布局结果（对齐 yinhe `Layout` 字段，像素单位）
#[derive(Debug, Clone)]
pub struct PianoLayout {
    /// 内容区（含键盘列）在画布内的本地矩形
    pub content_rect: Rectangle,
    /// 音乐区（横向 = 键盘右缘起，纵向 = 全宽）在画布本地坐标
    pub music_rect: Rectangle,
    /// 键盘条矩形（横向左列 / 纵向底条）
    pub keyboard_rect: Rectangle,
    /// 标尺矩形（横向顶条 / 纵向左列）
    pub ruler_rect: Rectangle,
    /// 内容区顶边 y
    pub content_y: f32,
    /// 内容区底边 y
    pub content_bottom: f32,
    /// 内容区逻辑宽/高（content_rect 尺寸，u32 取整）
    pub w: u32,
    pub h: u32,
    /// 物理像素宽/高（× pixels_per_point）
    pub pw: u32,
    pub ph: u32,
    /// 工程总 tick（含 padding，供 clamp）
    pub total_ticks: f64,
    /// 自动化面板总高（P3 占位，当前固定 0）
    pub panels_total_h: f32,
    /// 布局使用的方向
    pub orientation: Orientation,
}

impl PianoLayout {
    /// 复用 `ViewState` 的坐标变换 — tick → 画布本地 x（对齐 `ViewState::tick_to_x`）
    #[must_use]
    pub fn tick_to_x(&self, view: &ViewState, tick: f32) -> f32 {
        view.tick_to_x(tick)
    }

    /// 复用 `ViewState` 的坐标变换 — x → tick（对齐 `ViewState::x_to_tick`）
    #[must_use]
    pub fn x_to_tick(&self, view: &ViewState, x: f32) -> f32 {
        view.x_to_tick(x)
    }

    /// 复用 `ViewState` 的坐标变换 — key → 画布本地 y（对齐 `ViewState::key_to_y`）
    #[must_use]
    pub fn key_to_y(&self, view: &ViewState, key: u16) -> f32 {
        view.key_to_y(key)
    }

    /// 复用 `ViewState` 的坐标变换 — y → key（对齐 `ViewState::y_to_key`）
    #[must_use]
    pub fn y_to_key(&self, view: &ViewState, y: f32) -> u16 {
        view.y_to_key(y)
    }

    /// 主轴像素 → tick（方向感知，横向 X / 纵向 Y）
    #[must_use]
    pub fn main_px_to_tick(&self, view: &ViewState, main_px: f32) -> f64 {
        match self.orientation {
            Orientation::Horizontal => self.x_to_tick(view, main_px) as f64,
            Orientation::Vertical => {
                // 纵向：时间轴沿 Y，tick0 在顶，向下增大
                // ViewState 未提供纵向 tick 映射，此处以 zoom_x 复用（与横向一致语义）
                ((main_px + view.scroll_y) / view.zoom_x) as f64
            }
        }
    }

    /// 横轴像素（副轴 key 轴）：横向 = y，纵向 = x
    #[must_use]
    pub fn cross_px_to_key(&self, view: &ViewState, cross_px: f32) -> u8 {
        match self.orientation {
            Orientation::Horizontal => self.y_to_key(view, cross_px) as u8,
            Orientation::Vertical => {
                // 纵向：key 沿 X，key0 最左
                let k = ((cross_px + view.scroll_x) / view.zoom_y).floor() as i32;
                k.clamp(0, 127) as u8
            }
        }
    }

    /// key → 副轴像素（对齐 `view.key_to_cross_px` 语义）
    #[must_use]
    pub fn key_to_cross_px(&self, view: &ViewState, key: u8) -> f32 {
        match self.orientation {
            Orientation::Horizontal => self.key_to_y(view, key as u16),
            Orientation::Vertical => key as f32 * view.zoom_y - view.scroll_x,
        }
    }

    /// tick → 主轴像素
    #[must_use]
    pub fn tick_to_main_px(&self, view: &ViewState, tick: f64) -> f32 {
        match self.orientation {
            Orientation::Horizontal => self.tick_to_x(view, tick as f32),
            Orientation::Vertical => tick as f32 * view.zoom_x + 0.0 - view.scroll_y,
        }
    }

    /// 可见 tick 范围（复用 `ViewState` 的 scroll/zoom 计算可见区）
    #[must_use]
    pub fn visible_tick_range(&self, view: &ViewState, content_w: f32) -> (f64, f64) {
        let start = view.x_to_tick(view.keyboard_width) as f64;
        let end = view.x_to_tick(view.keyboard_width + content_w) as f64;
        (start.min(end), start.max(end))
    }

    /// 可见 key 范围（横向 y 轴 / 纵向 x 轴统一接口）
    #[must_use]
    pub fn visible_key_range(&self, view: &ViewState, content_h: f32) -> (u8, u8) {
        let lo = view.y_to_key(content_h) as u8;
        let hi = view.y_to_key(0.0).min(127) as u8;
        (lo.min(hi), lo.max(hi))
    }
}

/// 计算钢琴卷帘布局（方向感知，含 clamp）
///
/// 对应 `yinhe layout::compute_layout:30..131`，但：
/// - `view` 为 `&mut ViewState`（复用 lumino 坐标系，不另起 PianoRollView）
/// - `rect` 为 iced 本地 bounds（`Rectangle`），替代 egui `Rect`
/// - `panels_natural_h` 为上层自动化面板自然高度（P3 预留，当前传 0）
/// - `ppp` 为 `pixels_per_point`（物理像素比）
/// - `total_ticks` 为工程总 tick（含 padding）
/// 返回 `None` 表示音乐区像素尺寸为 0，无需后续渲染。
#[must_use]
pub fn compute_layout(
    view: &mut ViewState,
    rect: Rectangle,
    panels_natural_h: f32,
    ppp: f32,
    total_ticks: f64,
    orientation: Orientation,
) -> Option<PianoLayout> {
    let vertical = orientation == Orientation::Vertical;
    let kb_w = view.keyboard_width;
    let scrollbar_w = 12.0_f32;
    let scrollbar_h = 12.0_f32;
    let pr_bar_h = 28.0_f32;
    let ruler_h = view.ruler_height;

    let content_right_x = rect.x + rect.width - scrollbar_w;

    // 自动化面板可用高度上限（横/纵分支与 yinhe 等价，65% 上限）
    let avail_h = if vertical {
        (rect.height - pr_bar_h - scrollbar_h - kb_w).max(0.0)
    } else {
        (rect.height - ruler_h - pr_bar_h - scrollbar_h).max(0.0)
    };
    let panels_max_h = (avail_h * 0.65).max(0.0);
    let panels_total_h = panels_natural_h.min(panels_max_h);

    // 音乐区几何：与 yinhe layout 等价
    let (content_y, content_bottom, content_left_x, music_left_x) = if vertical {
        let top = rect.y + pr_bar_h;
        let keyboard_top = rect.y + rect.height - scrollbar_h - kb_w;
        let bottom = keyboard_top - panels_total_h;
        let left = rect.x + ruler_h;
        (top, bottom.max(top), left, left)
    } else {
        let top = rect.y + pr_bar_h + ruler_h;
        let bottom = top + (avail_h - panels_total_h).max(0.0);
        (top, bottom, rect.x, rect.x + kb_w)
    };

    let content_rect = Rectangle::new(
        Point::new(content_left_x, content_y),
        Size::new(
            (content_right_x - content_left_x).max(0.0),
            (content_bottom - content_y).max(0.0),
        ),
    );
    let music_rect = Rectangle::new(
        Point::new(music_left_x, content_y),
        Size::new(
            (content_right_x - music_left_x).max(0.0),
            (content_bottom - content_y).max(0.0),
        ),
    );

    let w = content_rect.width as u32;
    let h = content_rect.height as u32;
    let pw = (w as f32 * ppp) as u32;
    let ph = (h as f32 * ppp) as u32;

    if w == 0 || h == 0 {
        return None;
    }

    // 复用 ViewState 的 clamp 语义（通过 Viewport 封装或直接钳制 scroll）
    // 此处直接用 ViewState 的 scroll 边界：总 tick × zoom_x 为内容宽，可视宽为 content 宽
    let max_scroll_x = (total_ticks as f32 * view.zoom_x - content_rect.width).max(0.0);
    let total_key_h = view.visible_key_count as f32 * view.zoom_y;
    let viewport_h = content_rect.height;
    let max_scroll_y = (total_key_h - viewport_h).max(0.0);
    view.scroll_x = view.scroll_x.clamp(0.0, max_scroll_x);
    view.scroll_y = view.scroll_y.clamp(0.0, max_scroll_y);

    let keyboard_rect = if vertical {
        let kb_bottom = rect.y + rect.height - scrollbar_h;
        Rectangle::new(
            Point::new(content_rect.x, kb_bottom - kb_w),
            Size::new(content_rect.width, kb_w),
        )
    } else {
        Rectangle::new(
            Point::new(content_rect.x, content_rect.y),
            Size::new(kb_w, content_rect.height),
        )
    };

    let ruler_rect = if vertical {
        Rectangle::new(
            Point::new(rect.x, content_y),
            Size::new(ruler_h, content_rect.height),
        )
    } else {
        Rectangle::new(
            Point::new(rect.x + kb_w, rect.y + pr_bar_h),
            Size::new(content_rect.width, ruler_h),
        )
    };

    Some(PianoLayout {
        content_rect,
        music_rect,
        keyboard_rect,
        ruler_rect,
        content_y,
        content_bottom,
        w,
        h,
        pw,
        ph,
        total_ticks,
        panels_total_h,
        orientation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_view() -> ViewState {
        ViewState::default()
    }

    fn rect() -> Rectangle {
        Rectangle::new(Point::new(0.0, 0.0), Size::new(800.0, 600.0))
    }

    #[test]
    fn layout_some_when_non_empty() {
        let mut v = default_view();
        let l = compute_layout(
            &mut v,
            rect(),
            0.0,
            1.0,
            1920.0 * 100.0,
            Orientation::Horizontal,
        )
        .expect("非空画布应返回布局");
        assert!(l.w > 0 && l.h > 0);
        assert_eq!(l.orientation, Orientation::Horizontal);
    }

    #[test]
    fn tick_roundtrip_horizontal() {
        let mut v = default_view();
        let l = compute_layout(
            &mut v,
            rect(),
            0.0,
            1.0,
            1920.0 * 100.0,
            Orientation::Horizontal,
        )
        .expect("布局存在");
        let tick = 480.0_f32;
        let x = l.tick_to_x(&v, tick);
        let back = l.x_to_tick(&v, x);
        assert!((tick - back).abs() < 1e-3);
    }

    #[test]
    fn key_cross_roundtrip_vertical() {
        let mut v = default_view();
        let l = compute_layout(
            &mut v,
            rect(),
            0.0,
            1.0,
            1920.0 * 100.0,
            Orientation::Vertical,
        )
        .expect("布局存在");
        let key = 60u8;
        let px = l.key_to_cross_px(&v, key);
        let k2 = l.cross_px_to_key(&v, px + v.zoom_y * 0.5);
        assert_eq!(k2, key);
    }
}
