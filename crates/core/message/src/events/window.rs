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

/// GPU 兼容性检测结论（UI 展示与文案选择的唯一分类依据）。
///
/// 与 `lumino_gfx::device_check::GpuCheckFailure` 是**投影关系**而非等价关系：
/// gfx 侧失败类型携带诊断字符串，属图形栈内部细节；这里只保留 UI 需要区分的类别。
/// 映射集中在宿主层（`src/runner/gpu_check.rs`），因此 UI crate 不必依赖 gfx。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuCheckOutcome {
    /// 检测通过
    Passed,
    /// 未找到本平台要求的图形后端适配器
    NoAdapter,
    /// 找到适配器，但请求设备失败（驱动 / 特性不满足）
    DeviceRequestFailed,
    /// 设备创建成功，但离屏渲染或回读失败
    RenderFailed,
    /// 检测未在时限内完成（驱动可能挂起）
    TimedOut,
    /// 报告自相矛盾：既未通过又没有失败原因（正常路径不应出现）
    Unknown,
}

impl GpuCheckOutcome {
    /// 是否判定为通过（缓存回写与「检测通过」展示的唯一依据）
    pub const fn is_passed(self) -> bool {
        matches!(self, Self::Passed)
    }
}

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
    /// GPU 兼容性检查完成（Runner → 设置页，纯文本 + 结论分类，避免图形栈类型泄漏）
    GpuCheckFinished {
        /// 检测结论分类（UI 据此选择文案与按钮可用性）
        outcome: GpuCheckOutcome,
        /// 技术详情（多行文本）
        detail: String,
        /// 选中适配器指纹（Runner 据此回写启动缓存；无适配器 / 检测超时时为 `None`）
        fingerprint: Option<String>,
    },
    /// 文档端口布局变更（UI → Runner）：请求按新 `max_port` 重建实时合成输出布局
    MidiPortLayoutChanged {
        /// 文档当前最大端口（FF 21；0 = 单端口）
        max_port: u8,
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
    ///
    /// `fingerprint` 供 Runner 回写启动缓存（与启动门控共用同一缓存键，语义为
    /// 「最近一次检测结果」）；UI 侧消费 `outcome` / `detail`，是否通过由
    /// [`GpuCheckOutcome::is_passed`] 派生——不额外传 `passed`，避免两个字段不一致。
    pub fn gpu_check_finished(
        outcome: GpuCheckOutcome,
        detail: String,
        fingerprint: Option<String>,
    ) -> Self {
        Self::GpuCheckFinished {
            outcome,
            detail,
            fingerprint,
        }
    }
    /// 构造最大化事件
    pub const fn maximize() -> Self {
        Self::Lifecycle(lifecycle::Event::Maximize)
    }
    /// 构造最小化事件
    pub const fn minimize() -> Self {
        Self::Lifecycle(lifecycle::Event::Minimize)
    }

    /// 构造"文档端口布局变更"事件（端口编辑导致 `max_port` 变化时由 UI 发出；
    /// Runner 据此调用 `apply_midi_port_layout` 重建实时输出布局）。
    pub const fn midi_port_layout_changed(max_port: u8) -> Self {
        Self::MidiPortLayoutChanged { max_port }
    }
}
