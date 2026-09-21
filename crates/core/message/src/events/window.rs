pub mod audio;
pub mod collaboration;
pub mod dialog;
pub mod lifecycle;
pub mod sync;
pub mod track;
pub mod video;

mod collaboration_ctors;
mod dialog_ctors;
mod sync_ctors;

// Dialog variant 已 Box 化以减小 Event 枚举体积（同 Message 布局评审结论）。
// const fn 构造函数经由 const helper `Event::dialog()` 使用 `Box::new`（Rust ≥1.85 支持）。
#[derive(Debug, Clone)]
/// 窗口事件
pub enum Event {
    /// 窗口生命周期事件
    Lifecycle(lifecycle::Event),
    /// 对话框事件
    Dialog(Box<dialog::Event>),
    /// 协作事件
    Collaboration(collaboration::Event),
    /// 同步事件
    Sync(sync::Event),
    /// 音轨删除 / 恢复（与 .lmdeltrack 缓存交互）
    Track(track::Event),
    /// GPU 兼容性检查请求（设置页 → Runner）
    GpuCheckRun,
    /// GPU 兼容性检查完成（Runner → 设置页，纯文本避免图形栈类型泄漏）
    GpuCheckFinished {
        /// 是否通过
        passed: bool,
        /// 技术详情（多行文本）
        detail: String,
    },
}

impl Event {
    // ── 生命周期构造函数（直接构造，无需中间函数） ──

    /// 构造拖拽窗口事件
    pub const fn drag() -> Self {
        Self::Lifecycle(lifecycle::Event::Drag)
    }
    /// 构造关闭窗口事件
    pub const fn close() -> Self {
        Self::Lifecycle(lifecycle::Event::Close)
    }
    /// 构造切换最大化事件
    pub const fn toggle_maximize() -> Self {
        Self::Lifecycle(lifecycle::Event::ToggleMaximize)
    }

    /// 构造 GPU 兼容性检查请求事件
    pub const fn gpu_check_run() -> Self {
        Self::GpuCheckRun
    }

    /// 构造 GPU 兼容性检查完成事件
    pub fn gpu_check_finished(passed: bool, detail: String) -> Self {
        Self::GpuCheckFinished { passed, detail }
    }
    /// 构造最大化事件
    pub const fn maximize() -> Self {
        Self::Lifecycle(lifecycle::Event::Maximize)
    }
    /// 构造最小化事件
    pub const fn minimize() -> Self {
        Self::Lifecycle(lifecycle::Event::Minimize)
    }
}
