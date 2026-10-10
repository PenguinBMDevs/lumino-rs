//! 剪辑带时间轴 Canvas（对标钢琴卷帘「内容区自绘 + 外挂滚动条」结构）
//!
//! 自绘内容见 [`draw`]：秒标尺、圆角视频/音频轨道条（首尾裁剪把手）、
//! 走带指示线。交互见 [`interact`]：滚轮滚动/缩放、标尺点击与拖拽定位
//! （剪辑面板独立传输时钟）、素材条整体移动与首尾裁剪。

use iced_core::Rectangle;
use iced_core::mouse;
use iced_widget::canvas::{self, Geometry, Program};

use lumino_ui_core::constants::editor::{SCROLL_LINES_SCALE, SCROLL_MAX_DELTA};
use lumino_ui_core::state::video_clip_state::ClipTrackEdit;

use crate::editor::zoom::{fixed_ratio_from_viewport, zoom_factor_from_delta};
use crate::message::VideoClipAction;
use crate::{Message, Renderer, Theme};

mod draw;
mod interact;

pub use draw::{HitZone, TrackGeom};
pub use interact::TrackDrag;

use interact::seek_action;

/// 每秒基准像素密度（zoom=1 时）
pub const PIXELS_PER_SEC: f32 = 80.0;

/// 标尺区高度
pub const RULER_HEIGHT: f32 = 24.0;

/// 轨道条高度
pub const TRACK_HEIGHT: f32 = 48.0;

/// 轨道条间距
pub const TRACK_SPACING: f32 = 8.0;

/// 主刻度间隔（秒）
pub const MAJOR_INTERVAL: f32 = 5.0;

/// 次刻度间隔（秒）
pub const MINOR_INTERVAL: f32 = 1.0;

/// 时间轴 Canvas 数据（每帧视图重建时由面板写入）
pub struct TimelineCanvas {
    /// MIDI 实际时长（秒，同时是素材源长）
    pub duration_secs: f32,
    /// 当前水平缩放倍率
    pub zoom: f32,
    /// 当前水平滚动位置（像素）
    pub scroll_x: f32,
    /// Ctrl 键按下状态（Ctrl+滚轮 = 缩放，卷帘同款交互）
    pub ctrl_pressed: bool,
    /// 当前播放位置（秒，剪辑面板独立传输时钟）；走带线画在其对应屏幕坐标
    pub playhead_secs: f32,
    /// 是否正在播放（播放中禁用标尺点击/拖拽定位）
    pub is_playing: bool,
    /// 视频轨素材编辑（偏移/首尾裁剪）
    pub video_edit: ClipTrackEdit,
    /// 音频轨素材编辑（偏移/首尾裁剪）
    pub audio_edit: ClipTrackEdit,
}

/// Canvas 交互状态（scrub / 素材拖拽会话）
#[derive(Debug, Clone, Copy, Default)]
pub struct TimelineDragState {
    /// 正在按住标尺拖拽定位
    pub scrubbing: bool,
    /// 正在拖拽素材条（移动或裁剪）
    pub track_drag: Option<TrackDrag>,
}

impl TimelineCanvas {
    /// 时间轴内容秒长（秒）：素材源长与各轨素材带右缘取大。
    ///
    /// 素材带整体右移后右缘会越过源长，此时时间轴内容必须随之扩展——
    /// 否则越界部分滚不到（素材带拖出去就再也拖不回来）。
    pub fn content_secs(&self) -> f32 {
        lumino_ui_core::state::video_clip_state::timeline_content_secs(
            self.duration_secs,
            &[self.video_edit, self.audio_edit],
        )
    }

    /// 缩放后的内容总宽度（像素）
    pub fn content_width(&self) -> f32 {
        (self.content_secs() * PIXELS_PER_SEC * self.zoom).max(400.0)
    }

    /// 走带指示线的屏幕 x 坐标。
    ///
    /// 播放中滚动被自动跟随钉在 [`layout::PLAYHEAD_X`](super::layout::PLAYHEAD_X)；
    /// 暂停时可随手动滚动移出视口（返回 `None` 表示不绘制）。
    pub fn playhead_screen_x(&self, viewport_w: f32) -> Option<f32> {
        let x = self.playhead_secs * PIXELS_PER_SEC * self.zoom - self.scroll_x;
        (-1.0..=viewport_w + 1.0).contains(&x).then_some(x)
    }
}

/// 纯函数：普通滚轮增量的水平滚动位移（钳制单次最大幅度）
pub fn wheel_scroll_delta(delta_y: f32) -> f32 {
    (delta_y * SCROLL_LINES_SCALE).clamp(-SCROLL_MAX_DELTA, SCROLL_MAX_DELTA)
}

/// 纯函数：标尺点击位置 → 目标播放秒数（钳制到时间轴内容范围内）
pub fn ruler_click_secs(local_x: f32, scroll_x: f32, zoom: f32, content_secs: f32) -> f32 {
    let secs = (local_x + scroll_x) / (PIXELS_PER_SEC * zoom.max(f32::EPSILON));
    secs.clamp(0.0, content_secs.max(0.0))
}

impl Program<Message, Theme, Renderer> for TimelineCanvas {
    type State = TimelineDragState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        // ── 拖拽会话收尾（必须先于边界判定）──
        // 指针在画布外抬起时也要结束会话：早期 `return None` 会让
        // `track_drag` / `scrubbing` 残留，之后未按键的鼠标移动会继续改素材带
        // 长度（用户感知为「松手后画面还在动」）。
        if matches!(
            event,
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
        ) && (state.scrubbing || state.track_drag.is_some())
        {
            state.scrubbing = false;
            state.track_drag = None;
            return Some(canvas::Action::capture());
        }

        let pos = cursor.position()?;
        if !bounds.contains(pos) {
            return None;
        }

        // ── 拖拽会话事件（优先于新命中）──
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                return interact::begin_drag(self, state, pos, bounds);
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.scrubbing {
                    let local_x = (pos.x - bounds.x).clamp(0.0, bounds.width);
                    let secs =
                        ruler_click_secs(local_x, self.scroll_x, self.zoom, self.content_secs());
                    return Some(seek_action(secs));
                }
                if let Some(drag) = state.track_drag {
                    return interact::drag_move(self, pos, bounds, drag);
                }
            }
            _ => {}
        }

        match event {
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let local_x = pos.x - bounds.x;
                if self.ctrl_pressed {
                    // Ctrl+滚轮：以光标为锚点水平缩放（卷帘标尺区同款平滑步进）
                    let factor = zoom_factor_from_delta(delta)?;
                    Some(canvas::Action::publish(Message::VideoClip(
                        VideoClipAction::TimelineZoom {
                            zoom: self.zoom * factor,
                            fixed_ratio: fixed_ratio_from_viewport(local_x, 0.0, bounds.width),
                            viewport_w: bounds.width,
                        },
                    )))
                } else {
                    // 普通滚轮：水平滚动（时间轴仅有横向内容）
                    let dy = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => y * SCROLL_LINES_SCALE,
                        mouse::ScrollDelta::Pixels { y, .. } => *y,
                    };
                    let d = wheel_scroll_delta(dy);
                    if d.abs() < f32::EPSILON {
                        None
                    } else {
                        Some(canvas::Action::publish(Message::VideoClip(
                            VideoClipAction::TimelineScroll {
                                x: self.scroll_x + d,
                                viewport_w: bounds.width,
                            },
                        )))
                    }
                }
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        draw::draw_timeline(&mut frame, theme, bounds, self);
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        use mouse::Interaction;
        let Some(pos) = cursor.position() else {
            return Interaction::default();
        };
        if !bounds.contains(pos) {
            return Interaction::default();
        }
        // 拖拽中跟随模式光标
        if let Some(drag) = state.track_drag {
            return match drag.mode {
                HitZone::HandleStart | HitZone::HandleEnd => Interaction::ResizingHorizontally,
                HitZone::Body => Interaction::Grabbing,
            };
        }
        // 悬停提示：标尺十字 / 把手缩放 / 条身抓取
        if pos.y - bounds.y <= RULER_HEIGHT && !self.is_playing {
            return Interaction::Crosshair;
        }
        let pps_zoom = PIXELS_PER_SEC * self.zoom;
        let local = pos - iced_core::Vector::new(bounds.x, bounds.y);
        let (_, zone) = interact::hit_track(self, local, pps_zoom);
        match zone {
            Some(HitZone::HandleStart | HitZone::HandleEnd) => Interaction::ResizingHorizontally,
            Some(HitZone::Body) => Interaction::Grab,
            _ => Interaction::default(),
        }
    }
}

#[cfg(test)]
mod tests;
