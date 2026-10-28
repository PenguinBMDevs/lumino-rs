//! 对话框管理器
//!
//! 负责创建、管理和销毁对话框窗口。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

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
mod reconnect;

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
    /// ShapeDrawer 的待注入形状库目录
    pub shapes_dir: Option<PathBuf>,
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
    /// 窗口归属集合：**已创建**的对话框窗口 id（无论 UI 是否就绪）。
    ///
    /// REF-001 P0-1：事件分派必须按「窗口归属」判定，而不是按 `dialogs` 的
    /// 「UI 是否就绪」判定。阶段 1/2 期间窗口已存在但不在 `dialogs` 里，
    /// 若此时它的 `Resized`/`Moved`/`CloseRequested` 落到主窗口分支，会：
    /// - 把对话框的物理尺寸/位置写进主窗口持久化状态（`window_manager.rs`
    ///   的 `storage.ui_state.w/h/x/y`）⇒ 主窗口下次启动套用对话框的尺寸；
    /// - 把该窗口事件喂进主窗口的 iced 事件队列（光标/命中串台）；
    /// - `CloseRequested` 被当成主窗口关闭请求 ⇒ 关一个还在初始化的对话框会让整个应用退出。
    ///
    /// 认领点在阶段 1 建窗成功后（[`DialogManager::claim_window`]），释放点在
    /// [`DialogManager::close_dialog`] 与初始化失败清理路径。
    claimed_windows: HashSet<WindowId>,
}

impl Default for DialogManager {
    fn default() -> Self {
        Self::new()
    }
}
