use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::BufReader;
use std::path::{Path, PathBuf};

const CACHE_EXTENSION: &str = "lmt";

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
    cache_dir: PathBuf,
}

#[derive(Debug)]
pub struct TrackEventWindow {
    header: TrackCacheHeader,
    cache: TrackBasedCache,
    source_path: PathBuf,
    loaded_tracks: HashMap<usize, TrackEvents>,
    max_loaded_tracks: usize,
    access_order: Vec<usize>,
}

impl TrackBasedCache {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    pub fn new_in_program_dir() -> std::io::Result<Self> {
        let exe_dir = std::env::current_exe()?;
        let cache_dir = exe_dir
            .parent()
            .ok_or_else(|| std::io::Error::other("无法获取程序目录"))?
            .join("cache");

        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir)?;
        }

        Ok(Self::new(cache_dir))
    }

    fn compute_cache_key(path: &Path, file_modified: std::time::SystemTime) -> u64 {
        let mut hasher = DefaultHasher::new();
        path.to_string_lossy().hash(&mut hasher);
        file_modified.hash(&mut hasher);
        hasher.finish()
    }

    fn get_cache_dir(&self, key: u64) -> PathBuf {
        self.cache_dir.join(format!("{:016x}", key))
    }

    fn get_header_path(&self, key: u64) -> PathBuf {
        self.get_cache_dir(key).join("header.lmt")
    }

    fn get_track_path(&self, key: u64, track_index: usize) -> PathBuf {
        self.get_cache_dir(key)
            .join(format!("track_{:04x}.lmt", track_index))
    }

    pub fn get_header(&self, source_path: &Path) -> std::io::Result<Option<TrackCacheHeader>> {
        let metadata = fs::metadata(source_path)?;
        let modified = metadata.modified()?;
        let key = Self::compute_cache_key(source_path, modified);
        let header_path = self.get_header_path(key);

        if !header_path.exists() {
            return Ok(None);
        }

        let data = fs::read(&header_path)?;
        let header: TrackCacheHeader =
            bincode::deserialize(&data).map_err(std::io::Error::other)?;
        Ok(Some(header))
    }

    pub fn load_track(
        &self,
        source_path: &Path,
        track_index: usize,
    ) -> std::io::Result<Option<TrackEvents>> {
        let metadata = fs::metadata(source_path)?;
        let modified = metadata.modified()?;
        let key = Self::compute_cache_key(source_path, modified);
        let track_path = self.get_track_path(key, track_index);

        if !track_path.exists() {
            return Ok(None);
        }

        let file = File::open(&track_path)?;
        let reader = BufReader::new(file);
        let compressed = zstd::stream::decode_all(reader).map_err(std::io::Error::other)?;
        let track_events: TrackEvents =
            bincode::deserialize(&compressed).map_err(std::io::Error::other)?;

        Ok(Some(track_events))
    }

    pub fn build_cache_streaming(
        &self,
        source_path: &Path,
        stream: &mut crate::midi::MidiEventStream,
    ) -> std::io::Result<TrackCacheHeader> {
        if !self.cache_dir.exists() {
            fs::create_dir_all(&self.cache_dir)?;
        }

        let metadata = fs::metadata(source_path)?;
        let modified = metadata.modified()?;
        let key = Self::compute_cache_key(source_path, modified);
        let cache_dir = self.get_cache_dir(key);

        if cache_dir.exists() {
            fs::remove_dir_all(&cache_dir)?;
        }
        fs::create_dir_all(&cache_dir)?;

        let track_count = stream.track_count();
        let division = stream.division();
        let mut track_event_counts = Vec::with_capacity(track_count);
        let mut track_max_ticks = Vec::with_capacity(track_count);

        for track_idx in 0..track_count {
            let (event_count, max_tick) = {
                let events = stream
                    .read_track_events(track_idx)
                    .map_err(std::io::Error::other)?;

                let event_count = events.len() as u64;
                let max_tick = events.iter().map(|e| e.tick()).max().unwrap_or(0);

                let track_events = TrackEvents {
                    track_index: track_idx,
                    events,
                };

                let serialized =
                    bincode::serialize(&track_events).map_err(std::io::Error::other)?;
                let compressed = zstd::stream::encode_all(&mut &serialized[..], 3)
                    .map_err(std::io::Error::other)?;

                let track_path = self.get_track_path(key, track_idx);
                fs::write(&track_path, &compressed)?;

                (event_count, max_tick)
            };

            track_event_counts.push(event_count);
            track_max_ticks.push(max_tick);

            std::thread::yield_now();
        }

        let header = TrackCacheHeader {
            source_path: source_path.to_owned(),
            track_count: track_count as u32,
            division,
            track_event_counts,
            track_max_ticks,
        };

        let header_path = self.get_header_path(key);
        let header_data = bincode::serialize(&header).map_err(std::io::Error::other)?;
        fs::write(&header_path, &header_data)?;

        Ok(header)
    }

    pub fn open_window(
        &self,
        source_path: &Path,
        max_loaded_tracks: usize,
    ) -> std::io::Result<TrackEventWindow> {
        let header = self
            .get_header(source_path)?
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "缓存不存在"))?;

        Ok(TrackEventWindow {
            header,
            cache: self.clone(),
            source_path: source_path.to_owned(),
            loaded_tracks: HashMap::new(),
            max_loaded_tracks,
            access_order: Vec::new(),
        })
    }

    pub fn has_cache(&self, source_path: &Path) -> std::io::Result<bool> {
        Ok(self.get_header(source_path)?.is_some())
    }

    pub fn cache_single_track(
        &self,
        source_path: &Path,
        track_index: usize,
        track_events: &TrackEvents,
    ) -> std::io::Result<()> {
        if !self.cache_dir.exists() {
            fs::create_dir_all(&self.cache_dir)?;
        }

        let metadata = fs::metadata(source_path)?;
        let modified = metadata.modified()?;
        let key = Self::compute_cache_key(source_path, modified);
        let cache_dir = self.get_cache_dir(key);

        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir)?;
        }

        let serialized = bincode::serialize(track_events).map_err(std::io::Error::other)?;
        let compressed =
            zstd::stream::encode_all(&mut &serialized[..], 3).map_err(std::io::Error::other)?;

        let track_path = self.get_track_path(key, track_index);
        fs::write(&track_path, &compressed)?;

        Ok(())
    }

    pub fn finalize_cache(
        &self,
        source_path: &Path,
        track_count: u16,
        division: u16,
        track_event_counts: &[u64],
        track_max_ticks: &[u32],
    ) -> std::io::Result<()> {
        let metadata = fs::metadata(source_path)?;
        let modified = metadata.modified()?;
        let key = Self::compute_cache_key(source_path, modified);
        let _cache_dir = self.get_cache_dir(key);

        let header = TrackCacheHeader {
            source_path: source_path.to_owned(),
            track_count: track_count as u32,
            division,
            track_event_counts: track_event_counts.to_vec(),
            track_max_ticks: track_max_ticks.to_vec(),
        };

        let header_path = self.get_header_path(key);
        let header_data = bincode::serialize(&header).map_err(std::io::Error::other)?;
        fs::write(&header_path, &header_data)?;

        Ok(())
    }

    pub fn invalidate(&self, source_path: &Path) -> std::io::Result<()> {
        let metadata = fs::metadata(source_path)?;
        let modified = metadata.modified()?;
        let key = Self::compute_cache_key(source_path, modified);
        let cache_dir = self.get_cache_dir(key);

        if cache_dir.exists() {
            fs::remove_dir_all(cache_dir)?;
        }

        Ok(())
    }

    pub fn clear_all(&self) -> std::io::Result<()> {
        if !self.cache_dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                fs::remove_dir_all(path)?;
            }
        }

        Ok(())
    }

    pub fn cache_size(&self) -> std::io::Result<u64> {
        if !self.cache_dir.exists() {
            return Ok(0);
        }

        let mut total_size = 0u64;
        for entry in fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                for sub_entry in fs::read_dir(&path)? {
                    let sub_entry = sub_entry?;
                    if sub_entry
                        .path()
                        .extension()
                        .map(|e| e == CACHE_EXTENSION)
                        .unwrap_or(false)
                    {
                        total_size += sub_entry.metadata()?.len();
                    }
                }
            }
        }

        Ok(total_size)
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}

impl Clone for TrackBasedCache {
    fn clone(&self) -> Self {
        Self {
            cache_dir: self.cache_dir.clone(),
        }
    }
}

impl TrackEventWindow {
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
