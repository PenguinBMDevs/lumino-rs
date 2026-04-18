use std::collections::HashMap;
use std::path::PathBuf;

use super::types::{TrackBasedCache, TrackCacheHeader, TrackEvents};

#[derive(Debug)]
pub struct TrackEventWindow {
    header: TrackCacheHeader,
    cache: TrackBasedCache,
    source_path: PathBuf,
    loaded_tracks: HashMap<usize, TrackEvents>,
    max_loaded_tracks: usize,
    access_order: Vec<usize>,
}

impl TrackEventWindow {
    pub(crate) fn new(
        header: TrackCacheHeader,
        cache: TrackBasedCache,
        source_path: PathBuf,
        max_loaded_tracks: usize,
    ) -> Self {
        Self {
            header,
            cache,
            source_path,
            loaded_tracks: HashMap::new(),
            max_loaded_tracks,
            access_order: Vec::new(),
        }
    }

    pub fn header(&self) -> &TrackCacheHeader {
        &self.header
    }

    pub fn track_count(&self) -> u32 {
        self.header.track_count
    }

    pub fn get_track_events(&mut self, track_index: usize) -> std::io::Result<&TrackEvents> {
        if track_index as u32 >= self.header.track_count {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("音轨索引 {} 超出范围", track_index),
            ));
        }

        if !self.loaded_tracks.contains_key(&track_index) {
            if self.loaded_tracks.len() >= self.max_loaded_tracks
                && let Some(&oldest_key) = self.access_order.first()
            {
                self.loaded_tracks.remove(&oldest_key);
                self.access_order.retain(|&k| k != oldest_key);
            }

            let track_events = self
                .cache
                .load_track(&self.source_path, track_index)?
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("音轨 {} 缓存不存在", track_index),
                    )
                })?;

            self.loaded_tracks.insert(track_index, track_events);
            self.access_order.push(track_index);
        } else if let Some(pos) = self.access_order.iter().position(|&k| k == track_index) {
            self.access_order.remove(pos);
            self.access_order.push(track_index);
        }

        self.loaded_tracks
            .get(&track_index)
            .ok_or_else(|| std::io::Error::other("无法加载音轨"))
    }

    pub fn unload_all(&mut self) {
        self.loaded_tracks.clear();
        self.access_order.clear();
    }

    pub fn unload_track(&mut self, track_index: usize) {
        self.loaded_tracks.remove(&track_index);
        self.access_order.retain(|&k| k != track_index);
    }

    pub fn loaded_track_count(&self) -> usize {
        self.loaded_tracks.len()
    }

    pub fn total_track_count(&self) -> u32 {
        self.header.track_count
    }

    pub fn get_events_in_tick_range(
        &mut self,
        start_tick: u32,
        end_tick: u32,
    ) -> std::io::Result<Vec<crate::midi::MidiEvent>> {
        let mut result = Vec::new();

        for track_idx in 0..self.header.track_count as usize {
            let track_events = self.get_track_events(track_idx)?;
            for event in &track_events.events {
                let tick = event.tick();
                if tick >= start_tick && tick < end_tick {
                    result.push(event.clone());
                }
            }
        }

        Ok(result)
    }
}
