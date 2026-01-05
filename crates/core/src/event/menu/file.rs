#[derive(Debug, Clone)]
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
