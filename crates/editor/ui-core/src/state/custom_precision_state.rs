//! 自定义精度对话框状态

use lumino_core::shape::expr::eval_int;
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
    /// 最近一次提交失败的原因（None = 无错误）
    pub error: Option<String>,
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
            error: None,
        }
    }
}

impl CustomPrecisionDialogState {
    /// 创建一个默认的自定义精度对话框状态
    pub fn new() -> Self {
        Self::default()
    }

    /// 计算自定义精度对应的 tick 数（整数表达式精确求值）
    ///
    /// 输入允许 `960*4` / `(60+4)*16` 这类表达式；结果不为正或除法不精确时返回
    /// 可读错误，调用方应保留对话框并展示 `error`。
    pub fn calculate_ticks_checked(&self, ppq: u32) -> Result<f32, String> {
        let numerator = eval_int(&self.tuplet_count).map_err(|e| format!("三连音数：{e}"))?;
        let denominator = eval_int(&self.note_value).map_err(|e| format!("分音符：{e}"))?;
        let divisor = eval_int(&self.divisor).map_err(|e| format!("除数：{e}"))?;

        if numerator <= 0 || denominator <= 0 || divisor <= 0 {
            return Err("三连音数、分音符与除数都必须大于 0".to_string());
        }

        // 先算拍内 tick，再除除数；用 f64 承接中间值，最终必须是有限正数。
        let base_ticks = f64::from(ppq) * 4.0 * numerator as f64 / denominator as f64;
        let ticks = base_ticks / divisor as f64;
        if !ticks.is_finite() || ticks <= 0.0 {
            return Err("计算结果不是有限正数".to_string());
        }
        Ok(ticks as f32)
    }

    /// 兼容旧调用：失败时返回 `None`
    pub fn calculate_ticks(&self, ppq: u32) -> Option<f32> {
        self.calculate_ticks_checked(ppq).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_precision_accepts_integer_expressions() {
        let state = CustomPrecisionDialogState {
            tuplet_count: "3".into(),
            note_value: "4*2".into(),
            divisor: "2".into(),
            ..Default::default()
        };
        let ticks = state.calculate_ticks_checked(960).expect("表达式应可求值");
        assert_eq!(ticks, 720.0, "960*4*3/(8*2)");
    }

    #[test]
    fn test_custom_precision_rejects_invalid_and_keeps_error_out_of_calc() {
        let state = CustomPrecisionDialogState {
            note_value: "1/0".into(),
            ..Default::default()
        };
        assert!(state.calculate_ticks_checked(960).is_err());
        assert_eq!(state.note_value, "1/0", "求值失败不得改写原输入");
    }
}
