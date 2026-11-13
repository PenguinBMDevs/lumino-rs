//! 找回删除音轨对话框模型。

/// 找回删除音轨：标题 / 提示 / 表头 / 条目 / 底部按钮。
#[derive(Debug, Clone, Default)]
pub struct RecoverTrackModel {
    /// 顶部是否有 30px 旧版 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 标题「找回删除音轨」。
    pub title: String,
    /// 12px 提示行。
    pub hint: String,
    /// 表头三列。
    pub filename_header: String,
    /// 表头「删除时间」。
    pub deleted_header: String,
    /// 表头「音符总数」。
    pub notes_header: String,
    /// 列表条目（顺序即生产顺序）。
    pub entries: Vec<RecoverTrackRowModel>,
    /// 空态文案（80 高居中）。
    pub empty: String,
    /// 当前选中条目（按钮动作取 path / 编号）。
    pub selected: Option<RecoverTrackSelection>,
    /// 是否存在可操作条目（= selected 非空）。
    pub can_act: bool,
    /// 「恢复」按钮。
    pub restore: String,
    /// 「永久删除」按钮。
    pub delete: String,
    /// 「关闭」按钮。
    pub close: String,
}

/// 列表一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoverTrackRowModel {
    /// 文件名。
    pub filename: String,
    /// 删除时间（已格式化）。
    pub deleted_at: String,
    /// 音符总数（已格式化）。
    pub note_count: String,
    /// 是否选中。
    pub selected: bool,
}

/// 选中条目的动作载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoverTrackSelection {
    /// 缓存文件路径。
    pub path: String,
    /// 原轨道编号。
    pub original_index: usize,
    /// 轨道编号（永久删除用）。
    pub track_id: u16,
}
