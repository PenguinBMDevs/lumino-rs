//! 图片转 MIDI 面板的可调参数（DRAW-030）
//!
//! 面板字段全部映射到 `i2m-rs` `ConverterConfig`，混合模式映射到
//! `PaletteSource`（`spread` → Floyd–Steinberg、`pattern` → Bayer 4×4），
//! 不重复实现抖动算法。亮度/对比度/饱和度/锐化由本仓
//! `lumino_extras::image_adjust` 在调色板生成之前处理。
//!
//! 算法选项表（稳定 id / 显示名 / i2m-rs 枚举）见 [`i2m_tables`](super::i2m_tables)。

use i2m_rs::{ColorIdMethod, KeyMode, NoteLengthMode, PaletteSource, ResizeAlgorithm};
use lumino_message::{I2mConfigField, I2mSelectField};

use crate::right_sidebar::i2m_tables::{
    I2mBlend, blend_modes, color_id_methods, id_at, index_of, key_modes, note_length_modes,
    palette_algorithms, resize_algorithms,
};

/// 图片转 MIDI 转换配置（用户可自定义项）
///
/// 数值字段与输入框文本缓冲成对保存：文本缓冲用于 iced `TextInput`
/// 的无抖动编辑，数值为转换权威值；二者在有效输入时保持同步。
/// 所有下拉选择以**索引**保存（渲染期查表），持久化时转成稳定 id。
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
    /// 最长音符（像素行，0 = 不限）
    pub max_note_length: u32,
    /// 亮度百分比（-100..=100）
    pub brightness: i16,
    /// 对比度百分比（-100..=100）
    pub contrast: i16,
    /// 饱和度百分比（-100..=100）
    pub saturation: i16,
    /// 锐化百分比（0..=100）
    pub sharpen: i16,
    /// 调色板算法索引（指向 `palette_algorithms`）
    pub palette_index: usize,
    /// key 映射索引（指向 `key_modes`）
    pub key_mode_index: usize,
    /// 音符长度策略索引（指向 `note_length_modes`）
    pub note_length_mode_index: usize,
    /// 重采样算法索引（指向 `resize_algorithms`）
    pub resize_algorithm_index: usize,
    /// 颜色匹配口径索引（指向 `color_id_methods`）
    pub color_id_method_index: usize,
    /// 混合模式索引（指向 `blend_modes`）
    pub blend_index: usize,
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
    /// 最长音符输入框文本
    pub max_note_length_text: String,
    /// 亮度输入框文本
    pub brightness_text: String,
    /// 对比度输入框文本
    pub contrast_text: String,
    /// 饱和度输入框文本
    pub saturation_text: String,
    /// 锐化输入框文本
    pub sharpen_text: String,
}

impl Default for I2mConfig {
    /// 默认值与 i2m-rs `ConverterConfig::default()` 一致：
    /// A0-C8（21-108）、16 色、K-Means++、目标高度 0（保持比例）、每像素 1 tick。
    /// 新增参数一律取「不改变输出」的取值（不限长度、全部键、面积重采样、RGB
    /// 匹配、无抖动、无调色），保证接线前后逐音符一致。
    fn default() -> Self {
        Self {
            start_key: 21,
            end_key: 108,
            target_height: 0,
            ticks_per_pixel: 1,
            color_count: 16,
            max_note_length: 0,
            brightness: 0,
            contrast: 0,
            saturation: 0,
            sharpen: 0,
            palette_index: 0,
            key_mode_index: 0,
            note_length_mode_index: 0,
            resize_algorithm_index: 0,
            color_id_method_index: 0,
            blend_index: 0,
            start_key_text: "21".into(),
            end_key_text: "108".into(),
            target_height_text: "0".into(),
            ticks_per_pixel_text: "1".into(),
            color_count_text: "16".into(),
            max_note_length_text: "0".into(),
            brightness_text: "0".into(),
            contrast_text: "0".into(),
            saturation_text: "0".into(),
            sharpen_text: "0".into(),
        }
    }
}

impl I2mConfig {
    /// 应用数字字段的文本输入（仅接受数字/负号；空串或单独负号只保留输入缓冲）
    pub fn apply_text(&mut self, field: I2mConfigField, text: &str) {
        if !is_numeric_like(text) {
            return;
        }
        if text.is_empty() || text == "-" {
            self.set_text(field, text);
            return;
        }
        let parsed = match text.parse::<i64>() {
            Ok(value) => value,
            Err(_) => return,
        };
        match field {
            I2mConfigField::StartKey => {
                let v = parsed.clamp(0, 127) as u8;
                // 保持 start <= end：越界时联动提升 end
                if v > self.end_key {
                    self.end_key = v;
                    self.set_text(I2mConfigField::EndKey, &v.to_string());
                }
                self.start_key = v;
                self.set_text(I2mConfigField::StartKey, &v.to_string());
            }
            I2mConfigField::EndKey => {
                let v = parsed.clamp(0, 127) as u8;
                // 保持 start <= end：越界时联动压低 start
                if v < self.start_key {
                    self.start_key = v;
                    self.set_text(I2mConfigField::StartKey, &v.to_string());
                }
                self.end_key = v;
                self.set_text(I2mConfigField::EndKey, &v.to_string());
            }
            I2mConfigField::TargetHeight => {
                let v = parsed.clamp(0, 2048) as u32;
                self.target_height = v;
                self.set_text(I2mConfigField::TargetHeight, &v.to_string());
            }
            I2mConfigField::TicksPerPixel => {
                let v = parsed.clamp(1, 64) as u32;
                self.ticks_per_pixel = v;
                self.set_text(I2mConfigField::TicksPerPixel, &v.to_string());
            }
            I2mConfigField::ColorCount => {
                let v = parsed.clamp(1, 64) as usize;
                self.color_count = v;
                self.set_text(I2mConfigField::ColorCount, &v.to_string());
            }
            I2mConfigField::MaxNoteLength => {
                let v = parsed.clamp(0, 4096) as u32;
                self.max_note_length = v;
                self.set_text(I2mConfigField::MaxNoteLength, &v.to_string());
            }
            I2mConfigField::Brightness => {
                let v = parsed.clamp(-100, 100) as i16;
                self.brightness = v;
                self.set_text(I2mConfigField::Brightness, &v.to_string());
            }
            I2mConfigField::Contrast => {
                let v = parsed.clamp(-100, 100) as i16;
                self.contrast = v;
                self.set_text(I2mConfigField::Contrast, &v.to_string());
            }
            I2mConfigField::Saturation => {
                let v = parsed.clamp(-100, 100) as i16;
                self.saturation = v;
                self.set_text(I2mConfigField::Saturation, &v.to_string());
            }
            I2mConfigField::Sharpen => {
                let v = parsed.clamp(0, 100) as i16;
                self.sharpen = v;
                self.set_text(I2mConfigField::Sharpen, &v.to_string());
            }
        }
    }

    /// 应用下拉选择（索引越界忽略，避免面板与配置分叉）
    pub fn set_selection(&mut self, field: I2mSelectField, index: usize) {
        let valid = match field {
            I2mSelectField::Palette => index < palette_algorithms().len(),
            I2mSelectField::KeyMode => index < key_modes().len(),
            I2mSelectField::NoteLengthMode => index < note_length_modes().len(),
            I2mSelectField::ResizeAlgorithm => index < resize_algorithms().len(),
            I2mSelectField::ColorIdMethod => index < color_id_methods().len(),
            I2mSelectField::Blend => index < blend_modes().len(),
        };
        if !valid {
            return;
        }
        match field {
            I2mSelectField::Palette => self.palette_index = index,
            I2mSelectField::KeyMode => self.key_mode_index = index,
            I2mSelectField::NoteLengthMode => self.note_length_mode_index = index,
            I2mSelectField::ResizeAlgorithm => self.resize_algorithm_index = index,
            I2mSelectField::ColorIdMethod => self.color_id_method_index = index,
            I2mSelectField::Blend => self.blend_index = index,
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
            I2mConfigField::MaxNoteLength => &mut self.max_note_length_text,
            I2mConfigField::Brightness => &mut self.brightness_text,
            I2mConfigField::Contrast => &mut self.contrast_text,
            I2mConfigField::Saturation => &mut self.saturation_text,
            I2mConfigField::Sharpen => &mut self.sharpen_text,
        };
        *target = text.to_string();
    }

    /// 当前混合模式（越界回退「无抖动」）
    pub fn blend(&self) -> I2mBlend {
        blend_modes()
            .get(self.blend_index)
            .map(|(_, _, blend)| *blend)
            .unwrap_or(I2mBlend::None)
    }

    /// 当前调色板算法（索引越界时回退 K-Means++）
    pub fn palette_algorithm(&self) -> PaletteSource {
        palette_algorithms()
            .get(self.palette_index)
            .map(|(_, _, source)| source.clone())
            .unwrap_or(PaletteSource::KMeansPlusPlus)
    }

    /// 实际使用的调色板算法：混合模式优先于调色板下拉
    ///
    /// `spread` / `pattern` 在 `i2m-rs` 中就是「K-Means++ 调色板 + 抖动」的
    /// 组合变体，因此这里只做映射，不新增抖动实现。
    pub fn palette(&self) -> PaletteSource {
        match self.blend() {
            I2mBlend::Spread => PaletteSource::FloydSteinbergDither,
            I2mBlend::Pattern => PaletteSource::OrderedDither,
            I2mBlend::None => self.palette_algorithm(),
        }
    }

    /// 颜色后处理参数（面板百分比 → `Adjust` 的 -1..=1）
    pub fn adjust(&self) -> lumino_extras::Adjust {
        lumino_extras::Adjust {
            brightness: f32::from(self.brightness) / 100.0,
            contrast: f32::from(self.contrast) / 100.0,
            saturation: f32::from(self.saturation) / 100.0,
            sharpen: f32::from(self.sharpen) / 100.0,
        }
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
            key_mode: key_modes()
                .get(self.key_mode_index)
                .map(|(_, _, mode)| *mode)
                .unwrap_or(KeyMode::AllKeys),
            note_length_mode: note_length_modes()
                .get(self.note_length_mode_index)
                .map(|(_, _, mode)| *mode)
                .unwrap_or(NoteLengthMode::Unlimited),
            max_note_length: self.max_note_length,
            resize_algorithm: resize_algorithms()
                .get(self.resize_algorithm_index)
                .map(|(_, _, algorithm)| *algorithm)
                .unwrap_or(ResizeAlgorithm::AreaResampling),
            color_id_method: color_id_methods()
                .get(self.color_id_method_index)
                .map(|(_, _, method)| *method)
                .unwrap_or(ColorIdMethod::Rgb),
            ..Default::default()
        }
    }

    /// 导出为工程持久化结构（下拉索引 → 稳定 id）
    pub fn to_settings(&self) -> lumino_core::I2mSettings {
        lumino_core::I2mSettings {
            start_key: self.start_key,
            end_key: self.end_key,
            target_height: self.target_height,
            ticks_per_pixel: self.ticks_per_pixel,
            color_count: self.color_count as u32,
            max_note_length: self.max_note_length,
            brightness: self.brightness,
            contrast: self.contrast,
            saturation: self.saturation,
            sharpen: self.sharpen,
            palette: id_at(
                &palette_algorithms(),
                self.palette_index,
                "kmeans_plus_plus",
            ),
            key_mode: id_at(&key_modes(), self.key_mode_index, "all_keys"),
            note_length_mode: id_at(
                &note_length_modes(),
                self.note_length_mode_index,
                "unlimited",
            ),
            resize_algorithm: id_at(
                &resize_algorithms(),
                self.resize_algorithm_index,
                "area_resampling",
            ),
            color_id_method: id_at(&color_id_methods(), self.color_id_method_index, "rgb"),
            blend: id_at(&blend_modes(), self.blend_index, "none"),
        }
    }

    /// 从工程持久化结构恢复（稳定 id → 面板索引，未知 id 回退默认项）
    pub fn from_settings(settings: &lumino_core::I2mSettings) -> Self {
        // 数值先夹取，防止手改工程文件把面板撑爆
        let mut config = Self {
            start_key: settings.start_key.min(127),
            end_key: settings.end_key.min(127),
            target_height: settings.target_height.min(2048),
            ticks_per_pixel: settings.ticks_per_pixel.clamp(1, 64),
            color_count: (settings.color_count as usize).clamp(1, 64),
            max_note_length: settings.max_note_length.min(4096),
            brightness: settings.brightness.clamp(-100, 100),
            contrast: settings.contrast.clamp(-100, 100),
            saturation: settings.saturation.clamp(-100, 100),
            sharpen: settings.sharpen.clamp(0, 100),
            ..Self::default()
        };
        config.palette_index = index_of(&palette_algorithms(), &settings.palette);
        config.key_mode_index = index_of(&key_modes(), &settings.key_mode);
        config.note_length_mode_index = index_of(&note_length_modes(), &settings.note_length_mode);
        config.resize_algorithm_index = index_of(&resize_algorithms(), &settings.resize_algorithm);
        config.color_id_method_index = index_of(&color_id_methods(), &settings.color_id_method);
        config.blend_index = index_of(&blend_modes(), &settings.blend);
        config.sync_texts();
        config
    }

    /// 按当前数值重算全部输入框文本
    fn sync_texts(&mut self) {
        self.start_key_text = self.start_key.to_string();
        self.end_key_text = self.end_key.to_string();
        self.target_height_text = self.target_height.to_string();
        self.ticks_per_pixel_text = self.ticks_per_pixel.to_string();
        self.color_count_text = self.color_count.to_string();
        self.max_note_length_text = self.max_note_length.to_string();
        self.brightness_text = self.brightness.to_string();
        self.contrast_text = self.contrast.to_string();
        self.saturation_text = self.saturation.to_string();
        self.sharpen_text = self.sharpen.to_string();
    }
}

/// 仅允许「数字」「负号 + 数字」或空串（空串/单独负号用于继续输入）
fn is_numeric_like(text: &str) -> bool {
    if text.is_empty() || text == "-" {
        return true;
    }
    text.strip_prefix('-')
        .unwrap_or(text)
        .chars()
        .all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests;
