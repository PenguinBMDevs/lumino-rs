//! 音轨混音器 (Mixer) 模块
//!
//! 提供音量推子、声像控制、静音/Solo 等混音功能。
//! 每个音轨对应一个混音通道。

use iced_core::{Alignment, Color, Length, Padding};
use iced_widget::{button, column, container, row, slider, space, text, Column};
use crate::{Element, Theme, window};

/// 混音器音轨状态
#[derive(Debug, Clone)]
pub struct MixerTrackState {
    /// 音轨 ID
    pub track_id: usize,
    /// 音轨名称
    pub name: String,
    /// 音量 (0-127, MIDI CC7)
    pub volume: u8,
    /// 声像 (0-127, MIDI CC10, 64=center)
    pub pan: u8,
    /// 是否静音
    pub is_muted: bool,
    /// 是否 Solo
    pub is_solo: bool,
}

impl MixerTrackState {
    pub fn new(track_id: usize, name: String) -> Self {
        Self {
            track_id,
            name,
            volume: 100,
            pan: 64,
            is_muted: false,
            is_solo: false,
        }
    }
}

/// 混音器状态
#[derive(Debug, Clone)]
pub struct MixerState {
    /// 各音轨的混音状态
    pub tracks: Vec<MixerTrackState>,
    /// 主音量 (0-127)
    pub master_volume: u8,
    /// 是否打开
    pub is_open: bool,
    /// 面板宽度
    pub panel_width: f32,
}

impl Default for MixerState {
    fn default() -> Self {
        Self {
            tracks: Vec::new(),
            master_volume: 100,
            is_open: false,
            panel_width: 280.0,
        }
    }
}

impl MixerState {
    /// 从音轨列表更新混音器状态
    pub fn sync_from_tracks(&mut self, tracks: &[crate::sidebar::Track]) {
        for track in tracks {
            if let Some(existing) = self.tracks.iter_mut().find(|t| t.track_id == track.id) {
                existing.name = track.name.clone();
            } else {
                self.tracks.push(MixerTrackState::new(track.id, track.name.clone()));
            }
        }
        // 移除不存在的音轨
        self.tracks.retain(|t| tracks.iter().any(|st| st.id == t.track_id));
    }

    /// 渲染混音器视图
    pub fn view<'a>(&'a self, window: &'a window::Window) -> Element<'a> {
        let palette = window.theme.extended_palette();

        let mut col = Column::new()
            .spacing(4)
            .padding(Padding::new(8.0));

        // 标题
        let title = container(
            text("混音器")
                .size(14)
                .color(palette.background.neutral.text),
        )
        .padding([4, 0])
        .width(Length::Fill);

        col = col.push(title);
        col = col.push(space().height(4));

        // 各音轨推子
        for track in &self.tracks {
            col = col.push(self.render_track_strip(track, window));
        }

        // 主输出
        col = col.push(space().height(8));
        col = col.push(self.render_master_strip(window));

        container(col)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme: &Theme| {
                container::Style::default()
                    .background(palette.background.weakest.color)
            })
            .into()
    }

    /// 渲染单个音轨通道条
    fn render_track_strip<'a>(
        &'a self,
        track: &'a MixerTrackState,
        window: &'a window::Window,
    ) -> Element<'a> {
        let palette = window.theme.extended_palette();
        let bg = if track.is_muted {
            palette.background.weak.color
        } else if track.is_solo {
            palette.primary.weak.color
        } else {
            palette.background.weaker.color
        };

        container(
            column![
                // 音轨名称
                text(&track.name)
                    .size(11)
                    .width(Length::Fill)
                    .align_x(iced_core::alignment::Horizontal::Center),
                space().height(4),
                // 音量推子（垂直）
                slider(0..=127, track.volume, move |_| crate::Message::Null)
                    .step(1)
                    .width(Length::Fixed(100.0)),
                space().height(2),
                text(format!("{}", track.volume))
                    .size(10)
                    .color(palette.background.neutral.text),
                space().height(4),
                // 静音/Solo 按钮
                row![
                    button(
                        text("M")
                            .size(10)
                            .color(if track.is_muted { Color::from_rgb(1.0, 0.3, 0.3) } else { palette.background.neutral.text })
                    )
                    .padding([2, 8])
                    .style(move |_theme: &Theme, _status| {
                        button::Style {
                            border: iced_core::Border {
                                radius: 3.0.into(),
                                width: 0.0,
                                color: Color::TRANSPARENT,
                            },
                            ..Default::default()
                        }
                        .with_background(if track.is_muted { Color::from_rgba(1.0, 0.3, 0.3, 0.5) } else { palette.background.weak.color })
                    }),
                    space().width(4),
                    button(
                        text("S")
                            .size(10)
                            .color(if track.is_solo { Color::from_rgb(1.0, 0.8, 0.0) } else { palette.background.neutral.text })
                    )
                    .padding([2, 8])
                    .style(move |_theme: &Theme, _status| {
                        button::Style {
                            border: iced_core::Border {
                                radius: 3.0.into(),
                                width: 0.0,
                                color: Color::TRANSPARENT,
                            },
                            ..Default::default()
                        }
                        .with_background(if track.is_solo { palette.primary.base.color } else { palette.background.weak.color })
                    }),
                ]
                .spacing(2)
                .align_y(Alignment::Center),
            ]
            .spacing(2)
            .align_x(Alignment::Center),
        )
        .padding(6)
        .width(Length::Fixed(120.0))
        .style(move |_theme: &Theme| {
            container::Style::default()
                .background(bg)
                .border(iced_core::Border {
                    radius: 4.0.into(),
                    width: 1.0,
                    color: palette.background.strong.color,
                })
        })
        .into()
    }

    /// 渲染主输出通道
    fn render_master_strip<'a>(&'a self, window: &'a window::Window) -> Element<'a> {
        let palette = window.theme.extended_palette();

        container(
            column![
                text("主输出 Master")
                    .size(12)
                    .width(Length::Fill)
                    .align_x(iced_core::alignment::Horizontal::Center),
                space().height(4),
                slider(0..=127, self.master_volume, move |_| crate::Message::Null)
                    .step(1)
                    .width(Length::Fixed(100.0)),
                text(format!("{}", self.master_volume))
                    .size(10)
                    .color(palette.background.neutral.text),
            ]
            .spacing(2)
            .align_x(Alignment::Center),
        )
        .padding(8)
        .width(Length::Fixed(120.0))
        .style(move |_theme: &Theme| {
            container::Style::default()
                .background(palette.primary.weak.color)
                .border(iced_core::Border {
                    radius: 4.0.into(),
                    width: 1.0,
                    color: palette.primary.base.color,
                })
        })
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mixer_track_state_new() {
        let track = MixerTrackState::new(0, "Track 1".to_string());
        assert_eq!(track.track_id, 0);
        assert_eq!(track.name, "Track 1");
        assert_eq!(track.volume, 100);
        assert_eq!(track.pan, 64);
        assert!(!track.is_muted);
        assert!(!track.is_solo);
    }

    #[test]
    fn test_mixer_state_default() {
        let state = MixerState::default();
        assert!(!state.is_open);
        assert_eq!(state.master_volume, 100);
        assert!(state.tracks.is_empty());
    }

    #[test]
    fn test_mixer_state_sync_from_tracks() {
        let mut state = MixerState::default();
        let tracks = vec![
            crate::sidebar::Track {
                id: 0,
                name: "Track 1".to_string(),
                is_conductor: true,
                can_delete: false,
                is_muted: false,
                is_onion_skin_on: true,
            },
            crate::sidebar::Track {
                id: 1,
                name: "Track 2".to_string(),
                is_conductor: false,
                can_delete: true,
                is_muted: false,
                is_onion_skin_on: true,
            },
        ];

        state.sync_from_tracks(&tracks);
        assert_eq!(state.tracks.len(), 2);
        assert_eq!(state.tracks[0].name, "Track 1");
        assert_eq!(state.tracks[1].name, "Track 2");
    }

    #[test]
    fn test_mixer_state_sync_removes_old_tracks() {
        let mut state = MixerState::default();
        state.tracks.push(MixerTrackState::new(0, "Old".to_string()));
        state.tracks.push(MixerTrackState::new(1, "Removed".to_string()));

        let tracks = vec![
            crate::sidebar::Track {
                id: 0,
                name: "Track 1".to_string(),
                is_conductor: true,
                can_delete: false,
                is_muted: false,
                is_onion_skin_on: true,
            },
        ];

        state.sync_from_tracks(&tracks);
        assert_eq!(state.tracks.len(), 1);
        assert_eq!(state.tracks[0].name, "Track 1");
    }
}
