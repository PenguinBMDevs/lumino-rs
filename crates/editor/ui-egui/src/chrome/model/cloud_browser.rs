//! 云文件浏览面板与断连提醒模型。

/// 云文件浏览面板：工具栏 / 导航 / 新建 / 保存区 / 列表 / 提示。
#[derive(Debug, Clone, Default)]
pub struct CloudBrowserModel {
    /// 顶部是否有 30px iced 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 设备下拉（连接列表快照）。
    pub devices: Vec<CloudDeviceModel>,
    /// 下拉占位（「选择云存储」）。
    pub device_placeholder: String,
    /// 「刷新」。
    pub refresh: String,
    /// 「断开」。
    pub disconnect: String,
    /// 「粘贴」/「粘贴（剪切）」。
    pub paste: String,
    /// 剪贴板是否非空（false 时「粘贴」「清空剪贴板」置灰）。
    pub clipboard: bool,
    /// 「清空剪贴板」。
    pub clear_clipboard: String,
    /// 「← 返回」。
    pub back: String,
    /// 当前路径（空串显示「/」）。
    pub current_path: String,
    /// 新建文件夹输入框占位。
    pub new_folder_hint: String,
    /// 新建文件夹输入内容。
    pub new_folder_input: String,
    /// 「新建文件夹」按钮。
    pub new_folder: String,
    /// 保存模式下的上传目标提示区。
    pub save_panel: Option<CloudSavePanelModel>,
    /// 过滤后的条目。
    pub entries: Vec<CloudEntryModel>,
    /// 列表空态 / 加载态文案（`None` = 有条目）。
    pub placeholder: Option<String>,
    /// 操作结果提示（12px）。
    pub notice: Option<String>,
    /// 保存模式（隐藏下载/复制/剪切/重命名/删除）。
    pub save_mode: bool,
    /// 「确认删除？」前缀。
    pub confirm_delete: String,
    /// 「删除」按钮。
    pub delete: String,
    /// 「取消」按钮。
    pub cancel: String,
    /// 重命名输入框占位。
    pub rename_hint: String,
    /// 重命名输入内容（行内编辑态使用）。
    pub rename_input: String,
    /// 行尾按钮文案（本地化）。
    pub labels: CloudEntryLabels,
    /// 「确定」按钮。
    pub confirm: String,
}

/// 设备下拉一项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudDeviceModel {
    /// 在 `connections` 中的下标（动作回传后由生产侧取 id）。
    pub index: usize,
    /// 显示名（`名称 [协议] 在线/离线`）。
    pub label: String,
    /// 是否当前选中。
    pub selected: bool,
}

/// 保存模式下的上传目标区。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudSavePanelModel {
    /// 「将保存当前工程到：{path}」/「将上传素材 … 到：{path}」。
    pub title: String,
    /// 「保存到此处」/「上传中...」。
    pub save: String,
    /// 是否正在上传（按钮文案变化，仍可点击）。
    pub busy: bool,
}

/// 列表一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudEntryModel {
    /// 远程路径。
    pub path: String,
    /// 显示名（含 📁/📄 前缀）。
    pub name: String,
    /// 是否为目录。
    pub is_dir: bool,
    /// 文件大小（目录为空串）。
    pub size: String,
    /// 是否处于重命名编辑态。
    pub renaming: bool,
    /// 是否处于删除确认态。
    pub pending_delete: bool,
    /// 行尾操作按钮（保存模式为空）。
    pub actions: Vec<CloudEntryActionKind>,
}

/// 行尾操作按钮种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudEntryActionKind {
    /// 下载。
    Download,
    /// 复制。
    Copy,
    /// 剪切。
    Cut,
    /// 重命名。
    Rename,
    /// 删除。
    Delete,
}

impl CloudEntryActionKind {
    /// 按钮文案由生产侧本地化后回填的顺序标签（与 `actions` 同序）。
    pub fn label(self, labels: &CloudEntryLabels) -> &str {
        match self {
            Self::Download => &labels.download,
            Self::Copy => &labels.copy,
            Self::Cut => &labels.cut,
            Self::Rename => &labels.rename,
            Self::Delete => &labels.delete,
        }
    }
}

/// 条目按钮文案（本地化）。
#[derive(Debug, Clone, Default)]
pub struct CloudEntryLabels {
    /// 下载。
    pub download: String,
    /// 复制。
    pub copy: String,
    /// 剪切。
    pub cut: String,
    /// 重命名。
    pub rename: String,
    /// 删除。
    pub delete: String,
}

/// 云存储断连 / 失败提醒（`DialogType::CloudNotice`）。
#[derive(Debug, Clone, Default)]
pub struct CloudNoticeModel {
    /// 顶部是否有 30px iced 自制标题栏。
    pub titlebar: bool,
    /// 标题「云存储提示」。
    pub title: String,
    /// 提醒正文。
    pub message: String,
    /// 「知道了」按钮。
    pub dismiss: String,
}
