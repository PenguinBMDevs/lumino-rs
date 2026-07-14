//! 音符变速对话框状态

/// 音符变速对话框状态
#[derive(Debug, Clone)]
pub struct SpeedChangeDialogState {
    pub is_open: bool,
    /// 倍率输入字符串（支持分数格式如 "1/3"）
    pub factor_input: String,
}

impl SpeedChangeDialogState {
    pub fn new() -> Self {
        Self {
            is_open: false,
            factor_input: "0.5".to_string(),
        }
    }

    /// 解析倍率输入，支持小数和分数格式
    /// 返回解析成功的 f32 值
    pub fn parse_factor(&self) -> Option<f32> {
        let input = self.factor_input.trim();
        if input.is_empty() {
            return None;
        }

        // 尝试解析分数格式（如 "1/3"）
        if let Some(idx) = input.find('/') {
            let numerator = input[..idx].trim().parse::<f32>().ok()?;
            let denominator = input[idx + 1..].trim().parse::<f32>().ok()?;
            if denominator == 0.0 {
                return None;
            }
            let result = numerator / denominator;
            if result > 0.0 {
                return Some(result);
            }
            return None;
        }

        // 尝试解析小数格式
        let value = input.parse::<f32>().ok()?;
        if value > 0.0 { Some(value) } else { None }
    }
}

impl Default for SpeedChangeDialogState {
    fn default() -> Self {
        Self::new()
    }
}
