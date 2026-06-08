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
    /// 获取事件的人类可读显示名称
    pub fn display_name(&self) -> String {
        match self {
            Self::File(e) => e.display_name(),
            Self::Edit(e) => e.display_name(),
            Self::View(e) => e.display_name(),
            Self::Help(e) => e.display_name(),
        }
    }
}
