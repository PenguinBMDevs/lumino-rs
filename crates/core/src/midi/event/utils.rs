use std::path::Path;

use super::stream::MidiEventStream;

/// 解析全部MIDI事件（读取到内存，低内存占用）
pub fn parse_all_midi_events(path: &Path) -> Result<MidiEventStream, String> {
    MidiEventStream::from_path(path)
}
