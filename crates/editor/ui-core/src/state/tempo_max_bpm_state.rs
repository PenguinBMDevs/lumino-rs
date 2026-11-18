//! 自定义 Tempo BPM 上限对话框状态（独立 OS 窗口，DRAW-045）。
//!
//! 输入串与校验结果都归对话框自己：设置面板只保留已生效的
//! `tempo_max_bpm`，不再承载编辑中的临时态。

/// 「自定义 BPM 上限」对话框状态。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TempoMaxBpmDialogState {
    /// 对话框是否打开。
    pub is_open: bool,
    /// 输入框当前文本。
    pub input: String,
    /// 最近一次确认失败的原因（`None` = 无错误）。
    pub error: Option<String>,
}

impl TempoMaxBpmDialogState {
    /// 创建一个空的对话框状态。
    pub fn new() -> Self {
        Self::default()
    }

    /// 打开对话框，并用当前上限预填输入框。
    ///
    /// 与旧覆盖层口径一致：预填值取整（`{:.0}`），方便微调；错误状态清空。
    pub fn open(&mut self, current_bpm: f64) {
        self.is_open = true;
        self.input = format!("{current_bpm:.0}");
        self.error = None;
    }

    /// 关闭对话框。
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// 解析输入串为 BPM 上限。
    ///
    /// 校验口径与旧实现一致：只要求可解析为有限正数，不额外夹取上下限
    /// （预设下拉最高 65536，自定义入口历史上不设上限）。`NaN` / `inf`
    /// 虽然 `parse::<f64>()` 能通过，但会让 Tempo 面板的映射退化为 NaN，
    /// 因此这里显式拒绝。
    pub fn parse_bpm(&self) -> Result<f64, String> {
        let text = self.input.trim();
        if text.is_empty() {
            return Err(lumino_core::tr!("请输入 BPM 上限").to_string());
        }
        match text.parse::<f64>() {
            Ok(value) if value.is_finite() && value > 0.0 => Ok(value),
            _ => Err(lumino_core::tr!("请输入大于 0 的数字").to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_prefills_current_value_and_clears_error() {
        let mut state = TempoMaxBpmDialogState::new();
        state.error = Some("旧错误".to_string());
        state.open(700.0);
        assert!(state.is_open);
        assert_eq!(state.input, "700");
        assert!(state.error.is_none(), "打开时必须清掉上一次的错误");
    }

    #[test]
    fn test_parse_accepts_positive_finite() {
        let mut state = TempoMaxBpmDialogState::new();
        state.input = " 1234 ".to_string();
        assert_eq!(state.parse_bpm().expect("正整数应可解析"), 1234.0);

        state.input = "65536".to_string();
        assert_eq!(state.parse_bpm().expect("预设上限值应可解析"), 65536.0);
    }

    #[test]
    fn test_parse_rejects_empty_invalid_and_non_finite() {
        let mut state = TempoMaxBpmDialogState::new();
        for input in ["", "   ", "abc", "0", "-512", "NaN", "inf"] {
            state.input = input.to_string();
            assert!(
                state.parse_bpm().is_err(),
                "输入 {input:?} 不应被接受为 BPM 上限"
            );
        }
    }

    #[test]
    fn test_close_only_flips_flag() {
        let mut state = TempoMaxBpmDialogState::new();
        state.open(512.0);
        state.close();
        assert!(!state.is_open);
        assert_eq!(state.input, "512", "关闭不清空输入，重新打开前由 open 覆盖");
    }
}
