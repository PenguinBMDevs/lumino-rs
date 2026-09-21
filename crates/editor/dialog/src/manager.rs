//! 对话框管理器
//!
//! 负责创建、管理和销毁对话框窗口。

use std::collections::HashMap;

use lumino_core::BrushConfig;
use lumino_ui::state::root_state::DialogType;
use winit::window::WindowId;

use crate::window::DialogWindow;

// 子模块（按职责拆分，零逻辑变更）：
// - `lifecycle`：创建、分帧初始化、关闭与每帧更新
// - `queries`：存在性/引用查询与状态同步
// - `forwarding`：协作与视频导出事件转发
mod forwarding;
mod lifecycle;
mod queries;

/// 等待创建的对话框配置
#[derive(Debug, Clone)]
pub struct PendingDialog {
    /// 待创建对话框的类型
    pub dialog_type: DialogType,
    /// LoadConfirm 的 pending path
    pub pending_path: Option<String>,
    /// LoadConfirm 的待显示文件大小（MB）
    pub pending_size_mb: Option<f64>,
    /// ProjectSettings 的窗口标题
    pub pending_title: Option<String>,
    /// BrushSettings 的待注入画刷配置（打开时种入对话框草稿）
    pub pending_brush_config: Option<BrushConfig>,
}

/// 对话框管理器
///
/// 负责创建、管理和销毁对话框窗口。
pub struct DialogManager {
    /// 活跃的对话框窗口
    dialogs: HashMap<WindowId, DialogWindow>,
    /// 等待初始化的对话框配置
    pending_dialogs: Vec<PendingDialog>,
    /// 正在分帧初始化的对话框（已创建窗口，但 GFX/UI 尚未就绪）
    /// 元组保存对应的 PendingDialog，阶段 3 需要其中的配置数据。
    initializing: Vec<(DialogWindow, PendingDialog)>,
}

impl Default for DialogManager {
    fn default() -> Self {
        Self::new()
    }
}
