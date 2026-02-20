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
