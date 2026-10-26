//! 自定义精度对话框状态

use lumino_core::SnapSpec;
use lumino_core::shape::expr::eval_int;
use lumino_message::{DotType, TupletType};

/// 自定义精度对话框状态
#[derive(Debug, Clone)]
pub struct CustomPrecisionDialogState {
    /// 对话框是否打开
    pub is_open: bool,
    /// 连音分子（如三连音为 3）
    pub tuplet_count: String,
    /// 基础音符时值（如四分音符为 4）
    pub note_value: String,
    /// 连音类型
    pub tuplet_type: TupletType,
    /// 附点类型
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
            tuplet_count: "1".to_string(),
            note_value: "4".to_string(),
            tuplet_type: TupletType::None,
            dot_type: DotType::None,
            divisor: "1".to_string(),
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
        Ok(self.snap_spec()?.ticks(ppq as u16) as f32)
    }

    /// 把对话框字段换算成吸附规格。
    ///
    /// `div = 三连音数 × 除数`（DRAW-037 拍板口径）；附点只取 None/Single/Double。
    pub fn snap_spec(&self) -> Result<SnapSpec, String> {
        let notes = eval_int(&self.note_value).map_err(|e| format!("分音符：{e}"))?;
        let tuplet = eval_int(&self.tuplet_count).map_err(|e| format!("三连音数：{e}"))?;
        let divisor = eval_int(&self.divisor).map_err(|e| format!("除数：{e}"))?;
        if tuplet <= 0 || divisor <= 0 {
            return Err("三连音数与除数都必须大于 0".to_string());
        }
        let div = tuplet
            .checked_mul(divisor)
            .ok_or_else(|| "三连音数 × 除数溢出".to_string())?;
        if div > i64::from(u8::MAX) {
            return Err("三连音数 × 除数过大".to_string());
        }
        let notes = u8::try_from(notes).map_err(|_| "分音符超出 u8 范围".to_string())?;
        let div = u8::try_from(div).map_err(|_| "分割数超出 u8 范围".to_string())?;
        let dots = match self.dot_type {
            DotType::None | DotType::Tuplet => 0,
            DotType::Single => 1,
            DotType::Double => 2,
        };
        SnapSpec::note(notes, dots, div).map_err(|error| error.to_string())
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
            tuplet_count: "1".into(),
            note_value: "4*2".into(),
            divisor: "1".into(),
            ..Default::default()
        };
        let ticks = state.calculate_ticks_checked(960).expect("表达式应可求值");
        assert_eq!(ticks, 480.0, "960*4/8");

        let triplet = CustomPrecisionDialogState {
            tuplet_count: "3".into(),
            note_value: "4".into(),
            divisor: "1".into(),
            ..Default::default()
        };
        assert_eq!(
            triplet.snap_spec().expect("三连音"),
            SnapSpec::Note {
                notes: 4,
                dots: 0,
                div: 3
            }
        );
        let product = CustomPrecisionDialogState {
            tuplet_count: "3".into(),
            note_value: "4".into(),
            divisor: "2".into(),
            ..Default::default()
        };
        assert_eq!(
            product.snap_spec().expect("div 乘积"),
            SnapSpec::Note {
                notes: 4,
                dots: 0,
                div: 6
            }
        );
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
