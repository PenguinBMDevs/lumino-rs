use std::path::PathBuf;

pub(crate) const CACHE_EXTENSION: &str = "lmt";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrackCacheHeader {
    pub source_path: PathBuf,
    pub track_count: u32,
    pub division: u16,
    pub track_event_counts: Vec<u64>,
    pub track_max_ticks: Vec<u32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrackEvents {
    pub track_index: usize,
    pub events: Vec<crate::midi::MidiEvent>,
}

#[derive(Debug)]
pub struct TrackBasedCache {
    pub(crate) cache_dir: PathBuf,
}

pub use super::window_ops::TrackEventWindow;
