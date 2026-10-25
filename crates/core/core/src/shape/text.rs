//! 文字参数模型（DRAW-029）
//!
//! 只承载数据与校验，不参与光栅化；`LegacyBox` 与 `Parametric` 两条渲染路径
//! 共用本模型。默认值对应 Spiderweb `text.py` 的参数面，同时由文字工具决定
//! 是否进入兼容框填充模式（见后续 `TextLayoutMode`）。

use serde::{Deserialize, Serialize};

use crate::{CoreError, Result};

/// 默认字体族（与现有 `TextToolState::new` 保持一致）
pub const DEFAULT_FONT: &str = "Microsoft YaHei";

/// 每个 key 的纵向采样线数（阈值 5% 步进）
pub const SUB_ROWS: u16 = 20;

/// 字号单位
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextUnit {
    /// 字号以 keys 计
    #[default]
    Font,
    /// 大写字母高度占多少 keys
    Rows,
}

/// 文字排版路径
///
/// `LegacyBox` 保留当前“按文本框填满/拉伸”的逐像素行为；`Parametric` 才读取
/// [`TextParams`] 的 em 布局、阈值、grow 等参数。默认必须是 `LegacyBox`，
/// 否则未开参数面板的旧文本会改样。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextLayoutMode {
    /// 现有框填充兼容路径
    #[default]
    LegacyBox,
    /// 参数化排版路径
    Parametric,
}

/// 文本对齐
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    /// 左对齐
    #[default]
    Left,
    /// 居中
    Center,
    /// 右对齐
    Right,
}

/// 文字参数
///
/// 字段默认值对应 Spiderweb `TEXT_DEFAULTS`；是否实际走参数化布局由
/// `TextLayoutMode` 决定，不能仅凭本结构体判断。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextParams {
    /// 字体族名
    pub font: String,
    /// 字号（单位见 [`TextUnit`]）
    pub size: f32,
    /// 字号单位
    pub unit: TextUnit,
    /// 字重（100..=900）
    pub weight: u16,
    /// 是否斜体
    pub italic: bool,
    /// 字距
    pub tracking: f32,
    /// 行距百分比（100 = 单倍行距）
    pub leading: f32,
    /// 对齐方式
    pub align: TextAlign,
    /// 发声阈值（0..=100，按 key 高度采样）
    pub threshold: f32,
    /// 笔画粗细增量（keys，可负）
    pub grow: f32,
}

impl Default for TextParams {
    fn default() -> Self {
        Self {
            font: DEFAULT_FONT.to_string(),
            size: 24.0,
            unit: TextUnit::Font,
            weight: 400,
            italic: false,
            tracking: 0.0,
            leading: 100.0,
            align: TextAlign::Left,
            threshold: 50.0,
            grow: 0.0,
        }
    }
}

impl TextParams {
    /// 校验参数是否可参与排版/光栅化
    pub fn validate(&self) -> Result<()> {
        if self.font.trim().is_empty() {
            return Err(CoreError::InvalidArgument("文字字体不得为空".to_string()));
        }
        if !self.size.is_finite() || self.size <= 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "文字字号非法（需有限且为正，实际 {}）",
                self.size
            )));
        }
        if !(100..=900).contains(&self.weight) {
            return Err(CoreError::InvalidArgument(format!(
                "文字字重越界（允许 100..=900，实际 {}）",
                self.weight
            )));
        }
        if !self.tracking.is_finite() {
            return Err(CoreError::InvalidArgument(format!(
                "文字字距非法（需有限值，实际 {}）",
                self.tracking
            )));
        }
        if !self.leading.is_finite() || self.leading < 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "文字行距非法（需有限且非负，实际 {}）",
                self.leading
            )));
        }
        if !self.threshold.is_finite() || !(0.0..=100.0).contains(&self.threshold) {
            return Err(CoreError::InvalidArgument(format!(
                "文字阈值越界（允许 0..=100，实际 {}）",
                self.threshold
            )));
        }
        if !self.grow.is_finite() {
            return Err(CoreError::InvalidArgument(format!(
                "文字 grow 非法（需有限值，实际 {}）",
                self.grow
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_match_text_panel_spec() {
        let p = TextParams::default();
        assert_eq!(p.font, DEFAULT_FONT);
        assert_eq!(p.size, 24.0);
        assert_eq!(p.unit, TextUnit::Font);
        assert_eq!(p.weight, 400);
        assert!(!p.italic);
        assert_eq!(p.tracking, 0.0);
        assert_eq!(p.leading, 100.0);
        assert_eq!(p.align, TextAlign::Left);
        assert_eq!(p.threshold, 50.0);
        assert_eq!(p.grow, 0.0);
        assert!(p.validate().is_ok());
    }

    #[test]
    fn test_validate_rejects_invalid_fields() {
        let p = TextParams {
            font: String::new(),
            ..TextParams::default()
        };
        assert!(p.validate().is_err());

        let p = TextParams {
            size: 0.0,
            ..TextParams::default()
        };
        assert!(p.validate().is_err());

        let p = TextParams {
            weight: 99,
            ..TextParams::default()
        };
        assert!(p.validate().is_err());

        let p = TextParams {
            leading: -1.0,
            ..TextParams::default()
        };
        assert!(p.validate().is_err());

        let p = TextParams {
            threshold: 100.1,
            ..TextParams::default()
        };
        assert!(p.validate().is_err());

        let p = TextParams {
            grow: f32::NAN,
            ..TextParams::default()
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn test_unit_rows_and_alignment_are_serde_stable() {
        let params = TextParams {
            unit: TextUnit::Rows,
            align: TextAlign::Right,
            ..TextParams::default()
        };
        let encoded = bincode::serialize(&params).expect("TextParams 应可序列化");
        let decoded: TextParams = bincode::deserialize(&encoded).expect("TextParams 应可反序列化");
        assert_eq!(decoded, params);
    }
}
