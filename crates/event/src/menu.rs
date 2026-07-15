pub mod edit;
pub mod file;
pub mod help;
pub mod view;

/// 菜单事件
#[derive(Debug, Clone)]
pub enum Event {
    File(file::Event),
    Edit(edit::Event),
    View(view::Event),
    Help(help::Event),
}

impl Event {
    // ── 构造函数（替代 event! 宏） ──

    pub fn file(e: file::Event) -> Self {
        Self::File(e)
    }
    pub fn edit(e: edit::Event) -> Self {
        Self::Edit(e)
    }
    pub fn view(e: view::Event) -> Self {
        Self::View(e)
    }
    pub fn help(e: help::Event) -> Self {
        Self::Help(e)
    }
}
