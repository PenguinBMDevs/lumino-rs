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

/// 文字形状的持久化算子
///
/// 追加到 `ShapeOp` 末尾（bincode 旧数据安全）；文本框位置由
/// `ShapeModel.geometry.rect` 承载，`snap` 固定制字时的网格。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextOp {
    /// 原始文本（可多行）
    pub text: String,
    /// 文字参数
    pub params: TextParams,
    /// 制字时的排版模式
    pub layout: TextLayoutMode,
    /// 制字时的音符精度（tick）
    pub snap: f32,
    /// 制字时是否使用 key 范围合并采样
    pub merged: bool,
    /// 制字时是否纵向卷帘
    pub vertical: bool,
}

impl TextOp {
    /// 校验文字形状参数
    pub fn validate(&self) -> Result<()> {
        if self.text.is_empty() {
            return Err(CoreError::InvalidArgument(
                "文字形状文本不得为空".to_string(),
            ));
        }
        self.params.validate()?;
        if !self.snap.is_finite() || self.snap <= 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "文字形状精度非法（需有限且为正，实际 {}）",
                self.snap
            )));
        }
        Ok(())
    }
}

/// 冻结视图比例的文字形状算子（UI-023）
///
/// 与 [`TextOp`] 的唯一区别是 `k`：制字时的 beats per key。文字栅格必须按
/// `snap / (k · ppq)` 的屏幕格比例采样，否则字形会被压扁/拉宽；把该比例
/// 冻结在形状里，重烘焙才不会随当前缩放漂移。
///
/// 追加到 `ShapeOp` 末尾（bincode 旧数据安全）；旧 [`TextOp`] 走 4×4 兼容方块采样。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextOpV2 {
    /// 原始文本（可多行）
    pub text: String,
    /// 文字参数
    pub params: TextParams,
    /// 制字时的排版模式
    pub layout: TextLayoutMode,
    /// 制字时的音符精度（tick）
    pub snap: f32,
    /// 制字时是否使用 key 范围合并采样
    pub merged: bool,
    /// 制字时是否纵向卷帘
    pub vertical: bool,
    /// 制字时的 beats per key（`zoom_y / (ppq × zoom_x)`）
    pub k: f32,
}

impl TextOpV2 {
    /// 校验文字形状参数（口径同 [`TextOp::validate`]，额外要求 `k` 有限且为正）
    pub fn validate(&self) -> Result<()> {
        if self.text.is_empty() {
            return Err(CoreError::InvalidArgument(
                "文字形状文本不得为空".to_string(),
            ));
        }
        self.params.validate()?;
        if !self.snap.is_finite() || self.snap <= 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "文字形状精度非法（需有限且为正，实际 {}）",
                self.snap
            )));
        }
        if !self.k.is_finite() || self.k <= 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "文字形状 k 非法（需有限且为正，实际 {}）",
                self.k
            )));
        }
        Ok(())
    }
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

/// 字号单位换算（同字体）
///
/// `cap_ratio` = 大写字母高度 / em 尺寸（同字体恒定）。`Font` 单位是 em 尺寸，
/// `Rows` 单位是大写字母高度，故 `rows = font × cap_ratio`，反向除。
/// 结果 clamp 到面板范围 4..=96；`cap_ratio` 非法时按 0.7 兜底。
pub fn convert_size_unit(size: f32, from: TextUnit, to: TextUnit, cap_ratio: f32) -> f32 {
    // 面板字号范围：超出即夹到端点，避免排版/栅格出现 0 或天文字号。
    const MIN_SIZE: f32 = 4.0;
    const MAX_SIZE: f32 = 96.0;

    // NaN / 无穷没有可换算的语义，直接落到下界（clamp 会原样返回 NaN）。
    if !size.is_finite() {
        return MIN_SIZE;
    }
    if from == to {
        return size.clamp(MIN_SIZE, MAX_SIZE);
    }
    let ratio = if cap_ratio.is_finite() && cap_ratio > 0.0 {
        cap_ratio
    } else {
        0.7
    };
    let converted = if from == TextUnit::Font {
        size * ratio
    } else {
        size / ratio
    };
    converted.clamp(MIN_SIZE, MAX_SIZE)
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

    #[test]
    fn test_convert_size_unit_roundtrip() {
        let rows = convert_size_unit(8.0, TextUnit::Font, TextUnit::Rows, 0.7);
        assert!(
            (rows - 5.6).abs() <= 1e-4,
            "8 em × 0.7 应为 5.6 行，实际 {rows}"
        );
        let back = convert_size_unit(rows, TextUnit::Rows, TextUnit::Font, 0.7);
        assert!(
            (back - 8.0).abs() <= 1e-4,
            "Font→Rows→Font 应回到 8.0，实际 {back}"
        );
    }

    #[test]
    fn test_convert_size_unit_clamps() {
        assert_eq!(
            convert_size_unit(1000.0, TextUnit::Font, TextUnit::Rows, 0.7),
            96.0,
            "超大字号应被 clamp 到 96"
        );
        assert_eq!(
            convert_size_unit(1.0, TextUnit::Font, TextUnit::Rows, 0.7),
            4.0,
            "超小字号应被 clamp 到 4"
        );
        assert_eq!(
            convert_size_unit(f32::NAN, TextUnit::Font, TextUnit::Rows, 0.7),
            4.0,
            "非有限字号应走 4.0 兜底而不是 NaN"
        );
        // cap_ratio 非法（NaN / 0 / 负 / 无穷）一律按 0.7 兜底，不得 panic 或返回非有限值。
        for bad in [f32::NAN, 0.0, -0.7, f32::INFINITY] {
            let size = convert_size_unit(8.0, TextUnit::Font, TextUnit::Rows, bad);
            assert!(
                size.is_finite(),
                "非法 cap_ratio={bad} 应兜底 0.7，实际 {size}"
            );
            assert!((size - 5.6).abs() <= 1e-4, "兜底 0.7 应得 5.6，实际 {size}");
        }
    }

    #[test]
    fn test_text_op_v2_bincode_roundtrip() {
        let op = TextOpV2 {
            text: "LUMINO\nUI-023".to_string(),
            params: TextParams::default(),
            layout: TextLayoutMode::Parametric,
            snap: 480.0,
            merged: false,
            vertical: true,
            k: 0.4167,
        };
        op.validate().expect("默认参数 + 合法 k 应通过校验");
        let encoded = bincode::serialize(&op).expect("TextOpV2 应可序列化");
        let decoded: TextOpV2 = bincode::deserialize(&encoded).expect("TextOpV2 应可反序列化");
        assert_eq!(decoded, op, "TextOpV2 应逐字段往返（含冻结的 k）");

        // k 是新增字段的校验点：非有限/非正必须被拒绝，不能带病进栅格。
        for bad in [0.0_f32, -1.0, f32::NAN, f32::INFINITY] {
            let err = TextOpV2 {
                k: bad,
                ..op.clone()
            }
            .validate()
            .expect_err("非法 k 应被拒绝");
            assert!(
                format!("{err}").contains("文字形状 k 非法"),
                "实际错误：{err}"
            );
        }
    }
}
