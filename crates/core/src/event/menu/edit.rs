#[derive(Debug, Clone)]
/// 编辑事件
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
