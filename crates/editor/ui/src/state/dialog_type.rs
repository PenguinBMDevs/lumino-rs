//! 对话框类型。
//!
//! 从 `root_state.rs` 拆出：新增对话框（例如混音台）只动这里，RootState 本身
//! 保持聚焦于「状态字段」。外部仍可通过 `crate::state::root_state::DialogType`
//! 使用（`root_state.rs` 有 re-export），路径不变。

/// 对话框类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DialogType {
    /// 无对话框
    #[default]
    None,
    /// 自定义精度对话框
    CustomPrecision,
    /// 协作对话框
    Collaboration,
    /// 加载确认对话框
    LoadConfirm,
    /// 工程设置对话框
    ProjectSettings,
    /// 设置面板
    Settings,
    /// 素材删除确认（DRAW-045；独立 OS 窗口）
    MaterialDelete,
    /// 自定义 Tempo BPM 上限（DRAW-045；独立 OS 窗口）
    TempoMaxBpm,
    /// 音符变速对话框
    SpeedChange,
    /// 批量编辑对话框
    BatchEdit,
    /// 导出进度对话框
    ExportProgress,
    /// 视频导出对话框
    VideoExport,
    /// 内存监控对话框
    MemoryMonitor,
    /// 找回删除音轨
    RecoverTrack,
    /// 保存确认对话框（关闭工程 / 打开另一个工程 / 退出前的未保存更改确认）
    SaveConfirm,
    /// 自动保存恢复对话框（启动时询问是否恢复上次会话的自动保存；DEBT-12）
    AutosaveRecover,
    /// 云存储连接面板（地址/协议/用户名/密码）
    CloudConnect,
    /// 云文件浏览面板（仿资源管理器）
    CloudBrowser,
    /// 云存储断连/失败提醒面板（每次会话只弹一次）
    CloudNotice,
    /// 画刷「绘制行为」对话框（独立 OS 窗口）
    BrushSettings,
    /// 「音符画设置」对话框（独立 OS 窗口：各绘制工具设置的聚合入口）
    DrawSettings,
    /// 帮助窗口（F1；独立 OS 窗口）
    Help,
    /// 关于窗口（帮助菜单「关于」；独立 OS 窗口）
    About,
    /// 形状抽屉（独立 OS 窗口；DRAW-033）
    ShapeDrawer,
    /// 撤销历史（独立 OS 窗口；DRAW-035）
    History,
    /// 端口音色库（REND-005；独立 OS 窗口，从工程设置打开）
    PortSoundfonts,
    /// 混音台（独立 OS 窗口）
    Mixer,
}
