//! 测试与调试状态管理

/// 测试模式状态（FPS 监测等）
pub(crate) struct TestModeState {
    pub active: bool,
    pub start_time: Option<std::time::Instant>,
    pub duration: Option<u64>,
    pub fps_samples: Vec<f32>,
    pub last_fps_update: Option<std::time::Instant>,
    pub frame_count: u32,
}
