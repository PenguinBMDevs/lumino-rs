use iced_core::Color;

/// 黄金角比例（用于 HSV 颜色生成，最大化色相间距）
const GOLDEN_ANGLE: f32 = 0.381966;

/// 洋葱皮颜色配置
///
/// 提供16种默认颜色用于区分不同音轨的洋葱皮显示。
/// 音轨数量超过16时，自动使用黄金角 HSV 算法生成唯一颜色，
/// 确保任意数量音轨都有可区分的颜色。
#[derive(Debug, Clone)]
pub struct OnionSkinColors {
    colors: Vec<Color>,
    /// 透明度 (0.0 - 1.0)
    opacity: f32,
    /// 版本号，每次颜色/透明度变化时递增，用于渲染缓存失效
    version: u64,
}

impl Default for OnionSkinColors {
    fn default() -> Self {
        Self::new()
    }
}

impl OnionSkinColors {
    /// 创建默认的洋葱皮颜色配置
    ///
    /// 包含16种精心挑选的颜色，超出部分自动用 HSV 生成
    pub fn new() -> Self {
        let colors = vec![
            // 1. 珊瑚红
            Color::from_rgb(1.0, 0.5, 0.31),
            // 2. 天蓝色
            Color::from_rgb(0.53, 0.81, 0.92),
            // 3. 浅绿色
            Color::from_rgb(0.56, 0.93, 0.56),
            // 4. 紫罗兰
            Color::from_rgb(0.93, 0.51, 0.93),
            // 5. 金黄色
            Color::from_rgb(1.0, 0.84, 0.0),
            // 6. 青色
            Color::from_rgb(0.0, 1.0, 1.0),
            // 7. 热粉色
            Color::from_rgb(1.0, 0.41, 0.71),
            // 8. 橙色
            Color::from_rgb(1.0, 0.65, 0.0),
            // 9. 薰衣草
            Color::from_rgb(0.9, 0.9, 0.98),
            // 10. 酸橙绿
            Color::from_rgb(0.5, 1.0, 0.0),
            // 11. 三文鱼色
            Color::from_rgb(0.98, 0.5, 0.45),
            // 12. 薄荷绿
            Color::from_rgb(0.6, 1.0, 0.6),
            // 13. 薰衣草紫
            Color::from_rgb(0.8, 0.6, 1.0),
            // 14. 桃色
            Color::from_rgb(1.0, 0.8, 0.6),
            // 15. 天青蓝
            Color::from_rgb(0.6, 0.9, 1.0),
            // 16. 玫瑰色
            Color::from_rgb(1.0, 0.6, 0.8),
        ];

        Self {
            colors,
            opacity: 0.7, // 默认70%透明度
            version: 1,
        }
    }

    /// 获取当前颜色版本号（用于渲染缓存失效）
    #[inline]
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// 获取指定索引的颜色
    ///
    /// 前16个音轨使用预置调色板，之后的音轨自动用 HSV 生成唯一颜色。
    /// 生成的色相使用黄金角分布，保证任意数量都能均匀分散在色环上。
    pub fn get(&self, index: usize) -> Color {
        let color = if index < self.colors.len() {
            self.colors[index]
        } else {
            // 黄金角 HSV 生成：hue 按黄金角步进，确保色相均匀分布
            let hue = (index as f32 * GOLDEN_ANGLE).fract();
            let (r, g, b) = Self::hsv_to_rgb(hue, 0.85, 0.92);
            Color::from_rgb(r, g, b)
        };
        Color {
            r: color.r,
            g: color.g,
            b: color.b,
            a: self.opacity,
        }
    }

    /// 获取指定索引的颜色（原始颜色，不应用透明度）
    pub fn get_raw(&self, index: usize) -> Color {
        if index < self.colors.len() {
            self.colors[index]
        } else {
            let hue = (index as f32 * GOLDEN_ANGLE).fract();
            let (r, g, b) = Self::hsv_to_rgb(hue, 0.85, 0.92);
            Color::from_rgb(r, g, b)
        }
    }

    /// HSV → RGB 转换
    ///
    /// 适用于程序化生成颜色，保证任意 hue 值都能输出有效 RGB。
    fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> (f32, f32, f32) {
        let h = hue * 6.0;
        let c = value * saturation;
        let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
        let m = value - c;

        let sextant = h as u32 % 6;
        let (r, g, b) = match sextant {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };

        (r + m, g + m, b + m)
    }

    /// 设置指定索引的颜色
    ///
    /// 支持任意音轨索引，超出调色板范围时自动扩展。
    /// 扩展后的音轨将使用自定义颜色而非 HSV 生成色。
    pub fn set(&mut self, index: usize, color: Color) {
        if index >= self.colors.len() {
            self.colors.resize(index + 1, color);
        }
        self.colors[index] = color;
        self.version = self.version.wrapping_add(1);
    }

    /// 获取所有颜色
    pub fn colors(&self) -> &[Color] {
        &self.colors
    }

    /// 获取颜色数量
    pub fn len(&self) -> usize {
        self.colors.len()
    }

    /// 判断是否为空
    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }

    /// 获取透明度
    pub fn opacity(&self) -> f32 {
        self.opacity
    }

    /// 设置透明度
    ///
    /// # Arguments
    /// * `opacity` - 透明度值，范围 0.0（完全透明）到 1.0（完全不透明）
    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity.clamp(0.0, 1.0);
        self.version = self.version.wrapping_add(1);
    }

    /// 重置为默认颜色
    pub fn reset_to_default(&mut self) {
        *self = Self::new();
    }
}

/// 洋葱皮配置
///
/// 控制洋葱皮功能的开关和行为
#[derive(Debug, Clone)]
pub struct OnionSkinConfig {
    /// 是否启用洋葱皮
    pub enabled: bool,
    /// 颜色配置
    pub colors: OnionSkinColors,
    /// 是否显示所有音轨的洋葱皮
    pub show_all_tracks: bool,
    /// 指定显示哪些音轨的洋葱皮（为空时显示所有启用的）
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
    /// 创建新的洋葱皮配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 启用洋葱皮
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// 禁用洋葱皮
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// 切换洋葱皮开关状态
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    /// 检查洋葱皮是否启用
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// 设置是否显示所有音轨
    pub fn set_show_all_tracks(&mut self, show_all: bool) {
        self.show_all_tracks = show_all;
    }

    /// 添加可见音轨
    pub fn add_visible_track(&mut self, track_idx: usize) {
        if !self.visible_tracks.contains(&track_idx) {
            self.visible_tracks.push(track_idx);
        }
    }

    /// 移除可见音轨
    pub fn remove_visible_track(&mut self, track_idx: usize) {
        self.visible_tracks.retain(|&t| t != track_idx);
    }

    /// 清除可见音轨列表（显示所有启用的）
    pub fn clear_visible_tracks(&mut self) {
        self.visible_tracks.clear();
    }

    /// 检查音轨是否应该显示洋葱皮
    pub fn should_show_track(&self, track_idx: usize, track_onion_enabled: bool) -> bool {
        if !self.enabled {
            return false;
        }

        // 如果启用了 show_all_tracks，根据音轨自身的开关决定
        if self.show_all_tracks {
            return track_onion_enabled;
        }

        // 否则只在 visible_tracks 列表中的音轨才显示
        self.visible_tracks.contains(&track_idx)
    }

    /// 获取音轨的洋葱皮颜色
    pub fn get_track_color(&self, track_idx: usize) -> Color {
        self.colors.get(track_idx)
    }

    /// 设置音轨颜色
    pub fn set_track_color(&mut self, track_idx: usize, color: Color) {
        self.colors.set(track_idx, color);
    }

    /// 获取透明度
    pub fn opacity(&self) -> f32 {
        self.colors.opacity()
    }

    /// 设置透明度
    pub fn set_opacity(&mut self, opacity: f32) {
        self.colors.set_opacity(opacity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_onion_skin_colors() {
        let colors = OnionSkinColors::new();
        assert_eq!(colors.len(), 16);

        // 测试获取颜色
        let color = colors.get(0);
        assert!(color.a <= 1.0);
        assert!(color.a >= 0.0);

        // 测试调色板内颜色不重复
        for i in 0..16 {
            for j in (i + 1)..16 {
                let ci = colors.get(i);
                let cj = colors.get(j);
                let same = (ci.r - cj.r).abs() < 0.001
                    && (ci.g - cj.g).abs() < 0.001
                    && (ci.b - cj.b).abs() < 0.001;
                assert!(!same, "colors[{}] and colors[{}] should differ", i, j);
            }
        }

        // 测试超出调色板范围使用 HSV 生成（不再循环）
        let color_16 = colors.get(16);
        let color_0 = colors.get(0);
        let same_as_0 = (color_16.r - color_0.r).abs() < 0.001
            && (color_16.g - color_0.g).abs() < 0.001
            && (color_16.b - color_0.b).abs() < 0.001;
        assert!(!same_as_0, "track 16 should NOT reuse track 0 color");

        // 测试超大量音轨颜色仍然互异
        // 每个音轨独立计算颜色，黄金角保证颜色均匀分散
        for i in 0..800 {
            for j in (i + 1)..800 {
                let ci = colors.get(i);
                let cj = colors.get(j);
                let same = (ci.r - cj.r).abs() < f32::EPSILON
                    && (ci.g - cj.g).abs() < f32::EPSILON
                    && (ci.b - cj.b).abs() < f32::EPSILON;
                assert!(
                    !same,
                    "tracks {} and {} (out of 800) have identical color",
                    i, j
                );
            }
        }
    }

    #[test]
    fn test_onion_skin_config() {
        let mut config = OnionSkinConfig::new();
        assert!(config.is_enabled());

        config.disable();
        assert!(!config.is_enabled());

        config.toggle();
        assert!(config.is_enabled());

        // 测试音轨显示逻辑
        assert!(config.should_show_track(0, true));
        assert!(!config.should_show_track(0, false));

        config.disable();
        assert!(!config.should_show_track(0, true));
    }
}
