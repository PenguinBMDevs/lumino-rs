//! 颜料桶「分音符填充」面板状态（贴图标上方的工具栏小面板）

/// 分音符填充对话框状态
///
/// 只承载「输入框文本 + 开关」，实际切分档位落在
/// `LineToolState::fill_division`（编辑器侧），避免状态两份。
#[derive(Debug, Clone, Default)]
pub struct FillDivisionDialogState {
    /// 对话框是否打开
    pub is_open: bool,
    /// 输入框内容（x 分音符的 x；空串 = 恢复整块填充）
    pub value: String,
    /// 最近一次提交失败的原因（None = 无错误）
    pub error: Option<String>,
}

impl FillDivisionDialogState {
    /// 创建一个默认的对话框状态
    pub fn new() -> Self {
        Self::default()
    }

    /// 解析输入框内容 → 切分档位。
    ///
    /// - 空串 / `0` → `Ok(None)`（整块填充，不切分）；
    /// - 支持整数表达式（如 `8*2`）；
    /// - 非法或溢出 → `Err(String)`（调用方应保持对话框打开并展示错误）。
    pub fn parse(&self) -> Result<Option<u32>, String> {
        let s = self.value.trim();
        if s.is_empty() {
            return Ok(None);
        }
        let n = lumino_core::shape::expr::eval_int(s).map_err(|e| e.to_string())?;
        if n < 0 || n > i64::from(u32::MAX) {
            return Err("输入超出 u32 范围".to_string());
        }
        let n = n as u32;
        Ok(if n == 0 { None } else { Some(n) })
    }
}

#[cfg(test)]
mod tests {
    use super::FillDivisionDialogState;

    #[test]
    fn test_parse_empty_is_no_split() {
        let s = FillDivisionDialogState::new();
        assert_eq!(s.parse(), Ok(None), "留空 = 整块填充");
    }

    #[test]
    fn test_parse_zero_is_no_split() {
        let s = FillDivisionDialogState {
            is_open: true,
            value: "0".into(),
            ..Default::default()
        };
        assert_eq!(s.parse(), Ok(None), "0 = 整块填充");
    }

    #[test]
    fn test_parse_arbitrary_number() {
        // 需求：允许填写任意数字，不局限于 2 的幂
        for (input, expected) in [("16", 16u32), ("3", 3), ("7", 7), ("128", 128), ("1", 1)] {
            let s = FillDivisionDialogState {
                is_open: true,
                value: input.into(),
                ..Default::default()
            };
            assert_eq!(s.parse(), Ok(Some(expected)), "输入 {input}");
        }
    }

    #[test]
    fn test_parse_invalid() {
        let s = FillDivisionDialogState {
            is_open: true,
            value: "abc".into(),
            ..Default::default()
        };
        assert!(s.parse().is_err(), "非数字应报错");
    }

    #[test]
    fn test_parse_integer_expression() {
        let s = FillDivisionDialogState {
            is_open: true,
            value: "8*2".into(),
            ..Default::default()
        };
        assert_eq!(s.parse(), Ok(Some(16)), "整数表达式应精确求值");
    }

    #[test]
    fn test_parse_overflow_is_error() {
        let s = FillDivisionDialogState {
            is_open: true,
            value: "99999999999999".into(),
            ..Default::default()
        };
        assert!(s.parse().is_err(), "超出 u32 应报错");
    }
}
