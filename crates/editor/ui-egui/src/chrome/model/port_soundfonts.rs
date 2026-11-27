//! 端口音色库相关对话框模型（REND-005）。
//!
//! 从 `dialog.rs` 拆出：单文件保持 < 400 行；`dialog.rs` 只保留其它对话框模型。

/// 端口音色库对话框（REND-005）：整窗内容。
#[derive(Debug, Clone, Default)]
pub struct PortSoundfontsModel {
    /// 是否绘制 30px 自制标题栏。
    pub titlebar: bool,
    /// 窗口标题。
    pub title: String,
    /// 顶部说明。
    pub hint: String,
    /// 端口行（按端口升序）。
    pub rows: Vec<PortSoundfontsRowModel>,
    /// 确定按钮。
    pub ok: String,
    /// 取消按钮。
    pub cancel: String,
}

/// 端口音色库对话框中的一行。
#[derive(Debug, Clone, Default)]
pub struct PortSoundfontsRowModel {
    /// MIDI 端口号（动作回传用）。
    pub port: u8,
    /// 端口标签（端口 0 附带「全局（默认，可覆盖）」）。
    pub label: String,
    /// 当前路径（空 = 未配置）。
    pub path: String,
    /// 未配置时的占位文案。
    pub path_hint: String,
    /// 多路径回退链提示（v1 只显示首个）。
    pub extra: Option<String>,
    /// 「选择…」按钮文案。
    pub browse: String,
    /// 「清除」按钮文案。
    pub clear: String,
}

/// 「端口音色库未生效」提示窗（REND-005）：当前后端不支持时展示。
#[derive(Debug, Clone, Default)]
pub struct PortSoundfontsUnsupportedModel {
    /// 是否绘制 30px 自制标题栏。
    pub titlebar: bool,
    /// 窗口标题。
    pub title: String,
    /// 主提示（含后端名）。
    pub message: String,
    /// 降级说明（映射已保存、当前使用全局库）。
    pub detail: String,
    /// 「继续（使用全局音色库）」按钮。
    pub acknowledge: String,
    /// 「打开合成器设置」按钮。
    pub open_settings: String,
}
