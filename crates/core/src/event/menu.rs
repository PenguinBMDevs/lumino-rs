pub mod file;
pub mod edit;
pub mod view;
pub mod help;

#[derive(Debug, Clone)]
pub enum Event {
    File(file::Event),
    Edit(edit::Event),
    View(view::Event),
    Help(help::Event),
}
