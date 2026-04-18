use super::params::RenderParams;

/// 控制命令
#[derive(Debug)]
pub enum ControlCommand {
    /// 调整窗口大小
    Resize { width: u32, height: u32 },
    /// 停止渲染线程
    Shutdown,
}

/// 渲染命令（UI 线程 -> 渲染线程）
#[derive(Debug)]
pub enum RenderCommand {
    /// 渲染一帧
    Render(RenderParams),
    /// 控制命令
    Control(ControlCommand),
}
