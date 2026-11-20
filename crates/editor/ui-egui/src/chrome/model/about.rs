//! 关于窗口（`DialogType::About`）的只读模型。

/// 关于窗口模型：品牌行 + 版本行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AboutModel {
    /// 顶部是否有 30px 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 品牌行（如「Lumino - 新一代MIDI编辑器」，已本地化）。
    pub title: String,
    /// 版本行（如「版本 0.2.1」，已本地化）。
    pub version: String,
}
