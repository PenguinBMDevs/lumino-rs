/// 编辑菜单子模块
pub mod edit;
/// 文件菜单子模块
pub mod file;
/// 帮助菜单子模块
pub mod help;
/// 视图菜单子模块
pub mod view;

/// 菜单事件
#[derive(Debug, Clone)]
pub enum Event {
    /// 文件菜单事件
    File(file::Event),
    /// 编辑菜单事件
    Edit(edit::Event),
    /// 视图菜单事件
    View(view::Event),
    /// 帮助菜单事件
    Help(help::Event),
}

impl Event {
    // ── 构造函数（替代 event! 宏） ──

    /// 构造文件菜单事件
    pub fn file(e: file::Event) -> Self {
        Self::File(e)
    }
    /// 构造编辑菜单事件
    pub fn edit(e: edit::Event) -> Self {
        Self::Edit(e)
    }
    /// 构造视图菜单事件
    pub fn view(e: view::Event) -> Self {
        Self::View(e)
    }
    /// 构造帮助菜单事件
    pub fn help(e: help::Event) -> Self {
        Self::Help(e)
    }
}
