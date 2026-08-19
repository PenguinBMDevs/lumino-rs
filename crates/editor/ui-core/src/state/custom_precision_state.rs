//! 自定义精度对话框状态

use lumino_message::{DotType, TupletType};

/// 自定义精度对话框状态
#[derive(Debug, Clone)]
pub struct CustomPrecisionDialogState {
    /// 对话框是否打开
    pub is_open: bool,
    /// 连音符分子（如三连音为 3）
    pub tuplet_count: String,
    /// 基础音符时值（如四分音符为 4）
    pub note_value: String,
    /// 连音符类型
    pub tuplet_type: TupletType,
    /// 符点类型
    pub dot_type: DotType,
    /// 时值除数（额外除以的数值）
    pub divisor: String,
}

impl Default for CustomPrecisionDialogState {
    fn default() -> Self {
        Self {
            is_open: false,
            tuplet_count: "3".to_string(),
            note_value: "4".to_string(),
            tuplet_type: TupletType::Triplet,
            dot_type: DotType::None,
            divisor: "2".to_string(),
        }
    }
}

impl CustomPrecisionDialogState {
    /// 创建一个默认的自定义精度对话框状态
    pub fn new() -> Self {
        Self::default()
    }

    /// 计算自定义精度对应的 tick 数
    pub fn calculate_ticks(&self, ppq: u32) -> Option<f32> {
        let numerator = self.tuplet_count.parse::<f32>().ok()?;
        let denominator = self.note_value.parse::<f32>().ok()?;
        let divisor = self.divisor.parse::<f32>().ok()?;

        if denominator == 0.0 || divisor == 0.0 {
            return None;
        }

        // 计算基础 tick 数
        let base_ticks = (ppq as f32) * 4.0 * numerator / denominator;

        // 应用除数
        let ticks = base_ticks / divisor;

        Some(ticks)
    }
}
