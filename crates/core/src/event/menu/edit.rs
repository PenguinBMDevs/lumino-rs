#[derive(Debug, Clone)]
pub enum Event {
    Undo,
    Redo,
    /* */
    Cut,
    Copy,
    Paste,
    SelectAll,
    /* */
    Find,
}
