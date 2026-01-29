#[derive(Debug, Clone)]
/// 文件事件
pub enum Event {
    New,
    Open,
    Save,
    Close,
    /* */
    ImportMidi,
    /* */
    Settings,
    /* */
    Exit,
}
