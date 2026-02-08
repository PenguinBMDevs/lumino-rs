#[derive(Debug, Clone)]
pub enum Event {
    New,
    Open,
    Save,
    Close,
    /* */
    ImportMidi,
    MidiLoaded(crate::MidiInfo),
    MidiLoadError(String),
    /* */
    Settings,
    /* */
    Exit,
}
