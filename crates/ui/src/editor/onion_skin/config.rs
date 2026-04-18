use iced_core::Color;

use super::colors::OnionSkinColors;

/// 洋葱皮配置
///
/// 控制洋葱皮功能的开关和行为
#[derive(Debug, Clone)]
pub struct OnionSkinConfig {
    pub enabled: bool,
    pub colors: OnionSkinColors,
    pub show_all_tracks: bool,
    pub visible_tracks: Vec<usize>,
}

impl Default for OnionSkinConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            colors: OnionSkinColors::new(),
            show_all_tracks: true,
            visible_tracks: Vec::new(),
        }
    }
}

impl OnionSkinConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enable(&mut self) {
        self.enabled = true;
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_show_all_tracks(&mut self, show_all: bool) {
        self.show_all_tracks = show_all;
    }

    pub fn add_visible_track(&mut self, track_idx: usize) {
        if !self.visible_tracks.contains(&track_idx) {
            self.visible_tracks.push(track_idx);
        }
    }

    pub fn remove_visible_track(&mut self, track_idx: usize) {
        self.visible_tracks.retain(|&t| t != track_idx);
    }

    pub fn clear_visible_tracks(&mut self) {
        self.visible_tracks.clear();
    }

    pub fn should_show_track(&self, track_idx: usize, track_onion_enabled: bool) -> bool {
        if !self.enabled {
            return false;
        }

        if self.show_all_tracks {
            return track_onion_enabled;
        }

        self.visible_tracks.contains(&track_idx)
    }

    pub fn get_track_color(&self, track_idx: usize) -> Color {
        self.colors.get(track_idx)
    }

    pub fn set_track_color(&mut self, track_idx: usize, color: Color) {
        self.colors.set(track_idx, color);
    }

    pub fn opacity(&self) -> f32 {
        self.colors.opacity()
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.colors.set_opacity(opacity);
    }
}
