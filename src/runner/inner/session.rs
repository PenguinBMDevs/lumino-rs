//! 会话计时跟踪器
//!
//! 用于工程信息面板中"创建时间"和"创作总用时"的真实统计：
//! - 软件启动时记录 session_start_time
//! - MIDI 加载完成 + 洋葱皮贴图生成完成后设置 editing_start_time
//! - 打开工程设置对话框时计算累计编辑时间

/// 会话计时跟踪器
pub(crate) struct SessionTracker {
    /// 软件启动时间
    pub(crate) session_start_time: std::time::Instant,
    /// 编辑开始时间（MIDI 加载 + 洋葱皮贴图生成完成后设置）
    /// 如果没有加载 MIDI，从启动时间开始计算
    pub(crate) editing_start_time: Option<std::time::Instant>,
    /// 累计编辑时间（秒）—— 从 metadata 中加载的历史数据
    pub(crate) accumulated_editing_secs: f64,
    /// 工程创建时间（来自 MIDI 文件元数据或文件系统）
    pub(crate) created_at: Option<String>,
}

impl SessionTracker {
    pub(crate) fn new() -> Self {
        Self {
            session_start_time: std::time::Instant::now(),
            editing_start_time: None,
            accumulated_editing_secs: 0.0,
            created_at: None,
        }
    }

    /// 获取当前累计编辑时间（秒）
    ///
    /// 计算逻辑：
    /// - 如果已加载 MIDI 且洋葱皮生成完成（editing_start_time 已设置）：
    ///   accumulated + (now - editing_start_time)
    /// - 如果未加载 MIDI（editing_start_time 未设置）：
    ///   accumulated + (now - session_start_time)
    pub(crate) fn current_editing_secs(&self) -> f64 {
        let elapsed = if let Some(start) = self.editing_start_time {
            start.elapsed().as_secs_f64()
        } else {
            self.session_start_time.elapsed().as_secs_f64()
        };
        self.accumulated_editing_secs + elapsed
    }
}
