//! 内存监控对话框模型（只读，快照由生产侧采样）。

/// 内存监控：标题 / 三行总指标 / 子系统分类 / 说明 / 关闭。
#[derive(Debug, Clone, Default)]
pub struct MemoryMonitorModel {
    /// 顶部是否有 30px iced 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 标题「内存占用详情」。
    pub title: String,
    /// 总指标行（分配器总内存 / 系统 RSS / GPU 资源）。
    pub metrics: Vec<MemoryMetricModel>,
    /// 「按子系统分类」小节标题。
    pub grid_title: String,
    /// 子系统行（名称 + 已格式化数值）。
    pub rows: Vec<MemoryRowModel>,
    /// 无子系统数据时的文案。
    pub empty: String,
    /// 底部说明（GPU 计数口径）。
    pub note: String,
    /// 关闭按钮。
    pub close: String,
}

/// 总指标一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryMetricModel {
    /// 标签（「分配器追踪总内存」…）。
    pub label: String,
    /// 已格式化数值（「123.4 MB」）。
    pub value: String,
}

/// 子系统一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRowModel {
    /// 子系统名（`AllocTag::name()`）。
    pub name: String,
    /// 已格式化数值（「12.3 MB」）。
    pub value: String,
}
