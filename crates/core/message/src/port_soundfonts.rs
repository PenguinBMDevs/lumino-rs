//! 端口音色库对话框动作（REND-005 P4，独立窗口）。
//!
//! 对话框只回传「用户想做什么」，真正的映射构造、文件校验与实时应用由
//! Root / Runner 负责。

/// 端口音色库对话框动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortSoundfontsAction {
    /// 为指定端口选择音色库文件（异步文件对话框）。
    Browse(u8),
    /// 清除指定端口的覆盖（回退全局音色库）。
    Clear(u8),
    /// 确定：回传当前映射。
    Confirm,
    /// 取消：丢弃本次编辑。
    Cancel,
}
