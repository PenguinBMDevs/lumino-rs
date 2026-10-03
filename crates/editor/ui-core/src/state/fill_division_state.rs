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
}

impl FillDivisionDialogState {
    /// 创建一个默认的对话框状态
    pub fn new() -> Self {
        Self::default()
    }

    /// 解析输入框内容 → 切分档位。
    ///
    /// - 空串 / `0` → `Ok(None)`（整块填充，不切分）；
    /// - 非数字或溢出 → `Err(ParseIntError)`（调用方应保持对话框打开）。
    pub fn parse(&self) -> Result<Option<u32>, std::num::ParseIntError> {
        let s = self.value.trim();
        if s.is_empty() {
            return Ok(None);
        }
        let n: u32 = s.parse()?;
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
            };
            assert_eq!(s.parse(), Ok(Some(expected)), "输入 {input}");
        }
    }

    #[test]
    fn test_parse_invalid() {
        let s = FillDivisionDialogState {
            is_open: true,
            value: "abc".into(),
        };
        assert!(s.parse().is_err(), "非数字应报错");
    }

    #[test]
    fn test_parse_overflow_is_error() {
        let s = FillDivisionDialogState {
            is_open: true,
            value: "99999999999999".into(),
        };
        assert!(s.parse().is_err(), "超出 u32 应报错");
    }
}
