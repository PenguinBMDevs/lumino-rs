//! MIDI SMF 构建逻辑

use midly::{Format, Header, MetaMessage, Smf, Timing, Track, TrackEvent, TrackEventKind};

use super::types::{MidiExportData, MidiTrackData};

/// 构建 MIDI SMF 结构
pub fn build_midi_smf(data: &MidiExportData) -> Smf<'static> {
    let format = match data.options.format {
        0 => Format::SingleTrack,
        1 => Format::Parallel,
        2 => Format::Sequential,
        _ => Format::Parallel,
    };

    let timing = Timing::Metrical(data.options.ppqn.into());
    let header = Header::new(format, timing);

    let mut tracks: Vec<Track<'static>> = Vec::new();

    if data.options.format == 0 {
        let mut combined_track = build_combined_track(data);
        super::delta::convert_to_delta_times(&mut combined_track);
        tracks.push(combined_track);
    } else {
        let mut first_track = true;

        for track_data in &data.tracks {
            let mut track = build_track(track_data, first_track);
            super::delta::convert_to_delta_times(&mut track);
            tracks.push(track);
            first_track = false;
        }
    }

    Smf { header, tracks }
}

/// 构建合并轨道（格式 0）
fn build_combined_track(data: &MidiExportData) -> Track<'static> {
    let mut events: Vec<TrackEvent<'static>> = Vec::new();

    for track_data in &data.tracks {
        super::events::collect_track_events(track_data, &mut events, true);
    }

    events.sort_by_key(|e| e.delta);
    events
}

/// 构建单个轨道
fn build_track(track_data: &MidiTrackData, include_globals: bool) -> Track<'static> {
    let mut events: Vec<TrackEvent<'static>> = Vec::new();

    if let Some(ref name) = track_data.name {
        let name_bytes: &'static [u8] = Box::leak(name.clone().into_boxed_str().into_boxed_bytes());
        events.push(TrackEvent {
            delta: 0.into(),
            kind: TrackEventKind::Meta(MetaMessage::TrackName(name_bytes)),
        });
    }

    super::events::collect_track_events(track_data, &mut events, include_globals);

    events.push(TrackEvent {
        delta: 0.into(),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });

    events
}
