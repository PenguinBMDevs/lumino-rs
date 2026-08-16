//! 工程设置对话框状态

/// 工程设置对话框状态
#[derive(Debug, Clone)]
pub struct ProjectSettingsDialogState {
    pub is_open: bool,
    /// 项目名称
    pub title: String,
    /// BPM 速度 (字符串以便编辑)
    pub tempo: String,
    /// 版权信息
    pub copyright: String,
    /// 作者
    pub author: String,
    /// 创建日期 (格式化后的字符串)
    pub created_display: String,
    /// 累计创作时间 (秒)
    pub total_editing_time_seconds: f64,
    /// 拍号分子（字符串形式便于输入框绑定）
    pub time_signature_numerator: String,
    /// 拍号分母（字符串形式便于输入框绑定；人类可读，如 4、8、16）
    pub time_signature_denominator: String,
}

impl Default for ProjectSettingsDialogState {
    fn default() -> Self {
        Self {
            is_open: false,
            title: String::new(),
            tempo: "120".to_string(),
            copyright: String::new(),
            author: String::new(),
            created_display: String::new(),
            total_editing_time_seconds: 0.0,
            time_signature_numerator: "4".to_string(),
            time_signature_denominator: "4".to_string(),
        }
    }
}

impl ProjectSettingsDialogState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 重置为默认值（关闭工程 / 新建工程时调用，工程设置不得跨工程残留）
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// 格式化累计创作时间 (自适应单位)
    pub fn format_editing_time(&self) -> String {
        let total_seconds = self.total_editing_time_seconds;
        if total_seconds < 1.0 {
            return "不足 1 秒".to_string();
        }

        let days = (total_seconds / 86400.0) as u64;
        let hours = ((total_seconds % 86400.0) / 3600.0) as u64;
        let minutes = ((total_seconds % 3600.0) / 60.0) as u64;
        let seconds = (total_seconds % 60.0) as u64;

        if days > 0 {
            format!("{} 天 {} 小时 {} 分钟", days, hours, minutes)
        } else if hours > 0 {
            format!("{} 小时 {} 分钟", hours, minutes)
        } else if minutes > 0 {
            format!("{} 分钟 {} 秒", minutes, seconds)
        } else {
            format!("{} 秒", seconds)
        }
    }

    /// 解析 BPM 值 (20-10000)
    pub fn parse_tempo(&self) -> Option<f64> {
        let value = self.tempo.parse::<f64>().ok()?;
        if (20.0..=10000.0).contains(&value) {
            Some(value)
        } else {
            None
        }
    }

    /// 解析拍号，返回 (分子, 分母)
    pub fn parse_time_signature(&self) -> Option<(u8, u8)> {
        let numerator = self.time_signature_numerator.parse::<u8>().ok()?;
        let denominator = self.time_signature_denominator.parse::<u8>().ok()?;
        if numerator == 0 || denominator == 0 {
            return None;
        }
        Some((numerator, denominator))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_settings_dialog_reset_restores_defaults() {
        let mut state = ProjectSettingsDialogState::new();
        state.is_open = true;
        state.title = "我的工程".to_string();
        state.tempo = "96".to_string();
        state.copyright = "© 2026".to_string();
        state.author = "张三".to_string();
        state.created_display = "2026-07-01 10:00:00".to_string();
        state.total_editing_time_seconds = 3600.0;
        state.time_signature_numerator = "6".to_string();
        state.time_signature_denominator = "8".to_string();

        state.reset();

        // 工程设置属于工程级数据，关闭工程后必须恢复默认值，不得残留
        assert!(!state.is_open);
        assert_eq!(state.title, "");
        assert_eq!(state.tempo, "120");
        assert_eq!(state.copyright, "");
        assert_eq!(state.author, "");
        assert_eq!(state.created_display, "");
        assert_eq!(state.total_editing_time_seconds, 0.0);
        assert_eq!(state.time_signature_numerator, "4");
        assert_eq!(state.time_signature_denominator, "4");
    }
}
