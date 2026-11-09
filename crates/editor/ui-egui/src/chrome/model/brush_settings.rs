//! 画刷「绘制行为」对话框模型。

/// 画刷设置：标题 / 逐层音轨分配 / 空态提示 / 两个按钮。
#[derive(Debug, Clone, Default)]
pub struct BrushSettingsModel {
    /// 顶部是否有 30px iced 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 标题「画刷绘制行为设置」。
    pub title: String,
    /// 逐层分配（层数 = 全局粗细）。
    pub levels: Vec<BrushLevelModel>,
    /// 是否存在可选音轨（false 时列表整体替换为禁用提示）。
    pub has_tracks: bool,
    /// 无可用音轨时的提示行。
    pub empty: String,
    /// 保存按钮。
    pub save: String,
    /// 取消按钮。
    pub cancel: String,
}

/// 一层音轨分配。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrushLevelModel {
    /// 0-based 层下标（动作回传用）。
    pub index: usize,
    /// 层名（「层 1」…）。
    pub label: String,
    /// 下拉选项（首项为「默认（自动分配）」）。
    pub tracks: Vec<BrushTrackChoice>,
    /// 是否允许删除（层数 > 最小粗细）。
    pub can_remove: bool,
}

/// 音轨下拉项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrushTrackChoice {
    /// 音轨 id；`None` = 默认（自动分配）。
    pub id: Option<usize>,
    /// 显示名。
    pub label: String,
    /// 是否为当前选择。
    pub selected: bool,
}
