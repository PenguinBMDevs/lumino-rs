use super::params::RenderParams;
use crate::{KeyMode, OnionSkinNote};

/// 控制命令
#[derive(Debug)]
pub enum ControlCommand {
    /// 调整窗口大小
    Resize { width: u32, height: u32 },
    /// 停止渲染线程
    Shutdown,
    /// 启动洋葱皮概览贴图后台生成
    GenerateOnionSkin {
        /// 每轨音符列表
        notes: Vec<Vec<OnionSkinNote>>,
        /// 全曲时长（与音符时间同单位，对齐钢琴卷帘时为 tick）
        duration_ms: u32,
        /// 键位模式（决定贴图高度）
        key_mode: KeyMode,
    },
    /// 释放洋葱皮资源（关闭 MIDI 时调用）
    DisposeOnionSkin,
}

/// 渲染命令（UI 线程 -> 渲染线程）
#[derive(Debug)]
pub enum RenderCommand {
    /// 渲染一帧
    Render(Box<RenderParams>),
    /// 控制命令
    Control(ControlCommand),
}
