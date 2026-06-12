//! Toolbar 类型定义子模块
//!
//! 共享类型定义已迁移至 lumino-message crate，此处重新导出以保持路径兼容。

pub use lumino_message::{
    DotType, NotePrecision, SpeedFactor, Tool, TupletType,
};

/// 自定义精度对话框状态
#[derive(Debug, Clone)]
pub struct CustomPrecisionDialog {
    pub is_open: bool,
    /// 三连音数量（如 "3"）
    pub tuplet_count: String,
    /// 三连音类型
    pub tuplet_type: TupletType,
    /// 符点类型
    pub dot_type: DotType,
    /// 分音符值（如 "64"）
    pub note_value: String,
    /// 除数（如 "1"）
    pub divisor: String,
}

impl Default for CustomPrecisionDialog {
    fn default() -> Self {
        Self {
            is_open: false,
            tuplet_count: "3".to_string(),
            tuplet_type: TupletType::Triplet,
            dot_type: DotType::None,
            note_value: "64".to_string(),
            divisor: "1".to_string(),
        }
    }
}

impl CustomPrecisionDialog {
    /// 计算对应的tick值（基于PPQ）
    pub fn calculate_ticks(&self, ppq: u16) -> Option<f32> {
        let note_value = self.note_value.parse::<f32>().ok()?;
        let divisor = self.divisor.parse::<f32>().ok()?;

        if note_value == 0.0 || divisor == 0.0 {
            return None;
        }

        let base_ticks = (ppq as f32) * 4.0 / note_value;

        let tuplet_ratio = if self.dot_type != DotType::None {
            if let Ok(tuplet_count) = self.tuplet_count.parse::<f32>() {
                if tuplet_count > 1.0 {
                    (tuplet_count - 1.0) / tuplet_count
                } else {
                    1.0
                }
            } else {
                1.0
            }
        } else {
            1.0
        };

        let dot_multiplier = self.dot_type.multiplier();
        let final_ticks = base_ticks * tuplet_ratio * dot_multiplier / divisor;

        Some(final_ticks)
    }

    /// 获取显示文本
    pub fn display_text(&self) -> String {
        let mut text = String::new();
        if self.tuplet_count != "1" && !self.tuplet_count.is_empty() {
            text.push_str(&self.tuplet_count);
            text.push(' ');
        }
        text.push_str(&self.note_value);
        text.push_str("分音符");
        if self.divisor != "1" && !self.divisor.is_empty() {
            text.push_str(" / ");
            text.push_str(&self.divisor);
        }
        text
    }
}

/// 工具栏默认高度
pub const DEFAULT_HEIGHT: f32 = 72.0;
/// 最小高度
pub const MIN_HEIGHT: f32 = 56.0;
/// 最大高度
pub const MAX_HEIGHT: f32 = 200.0;
/// 拖拽手柄高度
pub const RESIZE_HANDLE_HEIGHT: f32 = 6.0;
