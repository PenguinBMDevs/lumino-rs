use iced_core::Color;

/// 洋葱皮颜色配置
///
/// 提供16种默认颜色用于区分不同音轨的洋葱皮显示
#[derive(Debug, Clone)]
pub struct OnionSkinColors {
    colors: Vec<Color>,
    opacity: f32,
}

impl Default for OnionSkinColors {
    fn default() -> Self {
        Self::new()
    }
}

impl OnionSkinColors {
    pub fn new() -> Self {
        let colors = vec![
            Color::from_rgb(1.0, 0.5, 0.31),
            Color::from_rgb(0.53, 0.81, 0.92),
            Color::from_rgb(0.56, 0.93, 0.56),
            Color::from_rgb(0.93, 0.51, 0.93),
            Color::from_rgb(1.0, 0.84, 0.0),
            Color::from_rgb(0.0, 1.0, 1.0),
            Color::from_rgb(1.0, 0.41, 0.71),
            Color::from_rgb(1.0, 0.65, 0.0),
            Color::from_rgb(0.9, 0.9, 0.98),
            Color::from_rgb(0.5, 1.0, 0.0),
            Color::from_rgb(0.98, 0.5, 0.45),
            Color::from_rgb(0.6, 1.0, 0.6),
            Color::from_rgb(0.8, 0.6, 1.0),
            Color::from_rgb(1.0, 0.8, 0.6),
            Color::from_rgb(0.6, 0.9, 1.0),
            Color::from_rgb(1.0, 0.6, 0.8),
        ];

        Self {
            colors,
            opacity: 0.4,
        }
    }

    pub fn get(&self, index: usize) -> Color {
        let color = self
            .colors
            .get(index % self.colors.len())
            .copied()
            .unwrap_or(self.colors[0]);
        Color {
            r: color.r,
            g: color.g,
            b: color.b,
            a: self.opacity,
        }
    }

    pub fn get_raw(&self, index: usize) -> Color {
        self.colors
            .get(index % self.colors.len())
            .copied()
            .unwrap_or(self.colors[0])
    }

    pub fn set(&mut self, index: usize, color: Color) {
        if index < self.colors.len() {
            self.colors[index] = color;
        }
    }

    pub fn colors(&self) -> &[Color] {
        &self.colors
    }

    pub fn len(&self) -> usize {
        self.colors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }

    pub fn opacity(&self) -> f32 {
        self.opacity
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity.clamp(0.0, 1.0);
    }

    pub fn reset_to_default(&mut self) {
        *self = Self::new();
    }
}
