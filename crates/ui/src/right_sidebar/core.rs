//! 右侧栏核心数据结构与常量

use lumino_message::I2mConfigField;
use once_cell::sync::Lazy;

/// 右侧栏图标栏宽度（固定，与左侧栏路由栏一致）
pub const ROUTE_BAR_WIDTH: f32 = 48.0;
/// 右侧栏面板默认宽度（与左侧栏面板一致）
pub const DEFAULT_PANEL_WIDTH: f32 = 200.0;
/// 右侧栏面板最小宽度
pub const MIN_PANEL_WIDTH: f32 = 150.0;
/// 右侧栏面板最大宽度
pub const MAX_PANEL_WIDTH: f32 = 900.0;
/// 右侧栏调整大小手柄宽度
pub const RESIZE_HANDLE_WIDTH: f32 = 6.0;

/// 可选调色板算法（中文名 + i2m-rs `PaletteSource`）
///
/// 跳过 i2m-rs 中标记为"回退"的变体（`OnlyWpfMedianCut`、`Optics`）与
/// 纯初始化变体 `OnlyKMeansPlusPlus`，其余全部暴露给用户。
pub static PALETTE_ALGORITHMS: Lazy<Vec<(&'static str, i2m_rs::PaletteSource)>> = Lazy::new(|| {
    vec![
        ("K-Means++", i2m_rs::PaletteSource::KMeansPlusPlus),
        ("K-Means", i2m_rs::PaletteSource::KMeans),
        ("流行色", i2m_rs::PaletteSource::Popularity),
        ("八叉树", i2m_rs::PaletteSource::Octree),
        ("方差分割", i2m_rs::PaletteSource::VarianceSplit),
        ("PCA 主成分", i2m_rs::PaletteSource::Pca),
        ("Max-Min", i2m_rs::PaletteSource::MaxMin),
        ("原生 K-Means", i2m_rs::PaletteSource::NativeKMeans),
        ("均值漂移", i2m_rs::PaletteSource::MeanShift),
        ("DBSCAN 密度", i2m_rs::PaletteSource::Dbscan),
        ("GMM 混合模型", i2m_rs::PaletteSource::Gmm),
        ("层次聚类", i2m_rs::PaletteSource::Hierarchical),
        ("谱聚类", i2m_rs::PaletteSource::Spectral),
        ("Lab K-Means", i2m_rs::PaletteSource::LabKMeans),
        (
            "Floyd-Steinberg 抖动",
            i2m_rs::PaletteSource::FloydSteinbergDither,
        ),
        ("Bayer 有序抖动", i2m_rs::PaletteSource::OrderedDither),
        ("固定位深调色板", i2m_rs::PaletteSource::FixedBitPalette),
    ]
});

/// 图片转 MIDI 转换配置（用户可自定义项）
///
/// 数值字段与输入框文本缓冲成对保存：文本缓冲用于 iced `TextInput`
/// 的无抖动编辑，数值为转换权威值；二者在有效输入时保持同步。
#[derive(Debug, Clone)]
pub struct I2mConfig {
    /// 最低 MIDI key（0-127）
    pub start_key: u8,
    /// 最高 MIDI key（0-127）
    pub end_key: u8,
    /// 目标高度（像素，0 = 保持宽高比）
    pub target_height: u32,
    /// 每像素行对应的 MIDI tick（>0）
    pub ticks_per_pixel: u32,
    /// 调色板颜色数（=生成音轨数）
    pub color_count: usize,
    /// 调色板算法索引（指向 `PALETTE_ALGORITHMS`）
    pub palette_index: usize,
    /// 最低 key 输入框文本
    pub start_key_text: String,
    /// 最高 key 输入框文本
    pub end_key_text: String,
    /// 目标高度输入框文本
    pub target_height_text: String,
    /// 每像素 tick 输入框文本
    pub ticks_per_pixel_text: String,
    /// 颜色数输入框文本
    pub color_count_text: String,
}

impl Default for I2mConfig {
    /// 默认值与 i2m-rs `ConverterConfig::default()` 一致：
    /// A0-C8（21-108）、16 色、K-Means++、目标高度 0（保持比例）、每像素 1 tick。
    fn default() -> Self {
        Self {
            start_key: 21,
            end_key: 108,
            target_height: 0,
            ticks_per_pixel: 1,
            color_count: 16,
            palette_index: 0,
            start_key_text: "21".into(),
            end_key_text: "108".into(),
            target_height_text: "0".into(),
            ticks_per_pixel_text: "1".into(),
            color_count_text: "16".into(),
        }
    }
}

impl I2mConfig {
    /// 应用数字字段的文本输入（仅接受纯数字；空串只保留输入缓冲不清数值）
    pub fn apply_text(&mut self, field: I2mConfigField, text: &str) {
        if !text.chars().all(|c| c.is_ascii_digit()) {
            return;
        }
        if text.is_empty() {
            self.set_text(field, "");
            return;
        }
        let parsed = text.parse::<u32>().unwrap_or(0);
        match field {
            I2mConfigField::StartKey => {
                let v = parsed.min(127) as u8;
                // 保持 start <= end：越界时联动提升 end
                if v > self.end_key {
                    self.end_key = v;
                    self.set_text(I2mConfigField::EndKey, &v.to_string());
                }
                self.start_key = v;
                self.set_text(I2mConfigField::StartKey, &v.to_string());
            }
            I2mConfigField::EndKey => {
                let v = parsed.min(127) as u8;
                // 保持 start <= end：越界时联动压低 start
                if v < self.start_key {
                    self.start_key = v;
                    self.set_text(I2mConfigField::StartKey, &v.to_string());
                }
                self.end_key = v;
                self.set_text(I2mConfigField::EndKey, &v.to_string());
            }
            I2mConfigField::TargetHeight => {
                let v = parsed.min(2048);
                self.target_height = v;
                self.set_text(I2mConfigField::TargetHeight, &v.to_string());
            }
            I2mConfigField::TicksPerPixel => {
                let v = parsed.clamp(1, 64);
                self.ticks_per_pixel = v;
                self.set_text(I2mConfigField::TicksPerPixel, &v.to_string());
            }
            I2mConfigField::ColorCount => {
                let v = parsed.clamp(1, 64) as usize;
                self.color_count = v;
                self.set_text(I2mConfigField::ColorCount, &v.to_string());
            }
        }
    }

    /// 写入指定字段的文本缓冲
    fn set_text(&mut self, field: I2mConfigField, text: &str) {
        let target = match field {
            I2mConfigField::StartKey => &mut self.start_key_text,
            I2mConfigField::EndKey => &mut self.end_key_text,
            I2mConfigField::TargetHeight => &mut self.target_height_text,
            I2mConfigField::TicksPerPixel => &mut self.ticks_per_pixel_text,
            I2mConfigField::ColorCount => &mut self.color_count_text,
        };
        *target = text.to_string();
    }

    /// 当前调色板算法（索引越界时回退 K-Means++）
    pub fn palette(&self) -> i2m_rs::PaletteSource {
        PALETTE_ALGORITHMS
            .get(self.palette_index)
            .map(|(_, source)| source.clone())
            .unwrap_or(i2m_rs::PaletteSource::KMeansPlusPlus)
    }

    /// 构建 i2m-rs 转换配置（归一化 key 顺序，其余沿用 i2m-rs 默认）
    pub fn to_converter_config(&self) -> i2m_rs::ConverterConfig {
        let (start, end) = if self.start_key <= self.end_key {
            (self.start_key, self.end_key)
        } else {
            (self.end_key, self.start_key)
        };
        i2m_rs::ConverterConfig {
            color_count: self.color_count,
            palette: self.palette(),
            start_key: start,
            end_key: end,
            target_height: self.target_height,
            ticks_per_pixel: self.ticks_per_pixel,
            ..Default::default()
        }
    }
}

/// 右侧栏状态
#[derive(Debug, Clone)]
pub struct RightSidebar {
    /// 面板是否可见
    pub panel_visible: bool,
    /// 面板宽度
    pub panel_width: f32,
    /// 是否正在拖拽调整宽度
    pub is_resizing: bool,
    /// 拖拽开始时的鼠标 X 坐标
    pub resize_start_x: f32,
    /// 拖拽开始时的面板宽度
    pub resize_start_width: f32,
    /// 用户通过文件对话框选中的待转换图片路径
    pub selected_image_path: Option<std::path::PathBuf>,
    /// 是否正在后台执行图片转 MIDI 转换
    pub converting: bool,
    /// i2m 转换配置（用户自定义项）
    pub config: I2mConfig,
}

impl RightSidebar {
    pub fn new() -> Self {
        Self {
            panel_visible: false,
            panel_width: DEFAULT_PANEL_WIDTH,
            is_resizing: false,
            resize_start_x: 0.0,
            resize_start_width: DEFAULT_PANEL_WIDTH,
            selected_image_path: None,
            converting: false,
            config: I2mConfig::default(),
        }
    }

    /// 计算右侧栏总宽度（图标栏 + 面板）
    pub fn width(&self) -> u32 {
        (ROUTE_BAR_WIDTH
            + if self.panel_visible {
                self.panel_width
            } else {
                0.0
            }) as u32
    }

    /// 切换面板显示/隐藏
    pub fn toggle_panel(&mut self) {
        self.panel_visible = !self.panel_visible;
    }

    /// 设置选中的图片路径（并确保面板展开以便查看结果）
    pub fn set_selected_image_path(&mut self, path: std::path::PathBuf) {
        self.selected_image_path = Some(path);
        self.panel_visible = true;
    }

    /// 开始拖拽调整面板宽度
    pub fn start_resize(&mut self, cursor_x: f32) {
        self.is_resizing = true;
        self.resize_start_x = cursor_x;
        self.resize_start_width = self.panel_width;
    }

    /// 更新拖拽位置
    pub fn update_resize_position(&mut self, cursor_x: f32) {
        if self.is_resizing {
            // 右侧栏的拖拽方向与左侧相反：鼠标左移增大面板
            let delta_x = self.resize_start_x - cursor_x;
            let new_width = self.resize_start_width + delta_x;
            self.panel_width = new_width.clamp(MIN_PANEL_WIDTH, MAX_PANEL_WIDTH);
        }
    }

    /// 结束拖拽
    pub fn end_resize(&mut self) {
        self.is_resizing = false;
    }
}

impl Default for RightSidebar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_matches_i2m_rs() {
        let cfg = I2mConfig::default();
        let cc = cfg.to_converter_config();
        assert_eq!(cc.start_key, 21);
        assert_eq!(cc.end_key, 108);
        assert_eq!(cc.color_count, 16);
        assert_eq!(cc.ticks_per_pixel, 1);
        assert_eq!(cc.target_height, 0);
        assert!(matches!(cc.palette, i2m_rs::PaletteSource::KMeansPlusPlus));
    }

    #[test]
    fn test_apply_text_rejects_non_digits() {
        let mut cfg = I2mConfig::default();
        cfg.apply_text(I2mConfigField::TicksPerPixel, "abc");
        assert_eq!(cfg.ticks_per_pixel, 1);
        assert_eq!(cfg.ticks_per_pixel_text, "1");
    }

    #[test]
    fn test_apply_text_clamps_ranges() {
        let mut cfg = I2mConfig::default();
        cfg.apply_text(I2mConfigField::StartKey, "200");
        assert_eq!(cfg.start_key, 127);
        cfg.apply_text(I2mConfigField::TicksPerPixel, "0");
        assert_eq!(cfg.ticks_per_pixel, 1);
        cfg.apply_text(I2mConfigField::ColorCount, "0");
        assert_eq!(cfg.color_count, 1);
        cfg.apply_text(I2mConfigField::TargetHeight, "9999");
        assert_eq!(cfg.target_height, 2048);
    }

    #[test]
    fn test_apply_text_keeps_start_le_end() {
        let mut cfg = I2mConfig::default();
        // start 超过 end → end 联动提升，保持 start <= end
        cfg.apply_text(I2mConfigField::StartKey, "120");
        assert_eq!(cfg.start_key, 120);
        assert_eq!(cfg.end_key, 120);
        // end 低于 start → start 联动压低
        cfg.apply_text(I2mConfigField::EndKey, "30");
        assert_eq!(cfg.end_key, 30);
        assert_eq!(cfg.start_key, 30);
    }

    #[test]
    fn test_empty_text_keeps_value() {
        let mut cfg = I2mConfig::default();
        cfg.apply_text(I2mConfigField::TicksPerPixel, "");
        assert_eq!(cfg.ticks_per_pixel, 1);
        assert_eq!(cfg.ticks_per_pixel_text, "");
    }

    #[test]
    fn test_to_converter_config_normalizes_key_order() {
        let mut cfg = I2mConfig::default();
        cfg.start_key = 100;
        cfg.end_key = 50;
        let cc = cfg.to_converter_config();
        assert_eq!(cc.start_key, 50);
        assert_eq!(cc.end_key, 100);
    }

    #[test]
    fn test_palette_fallback_on_out_of_range_index() {
        let cfg = I2mConfig {
            palette_index: 999,
            ..Default::default()
        };
        assert!(matches!(
            cfg.palette(),
            i2m_rs::PaletteSource::KMeansPlusPlus
        ));
    }
}
