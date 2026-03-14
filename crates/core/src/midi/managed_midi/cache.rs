//! 磁盘缓存管理

use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

use crate::midi::MidiEvent;

/// 压缩级别 (1-22, 越高压缩率越好但越慢)
const COMPRESSION_LEVEL: i32 = 3;

/// 磁盘缓存管理器
#[derive(Debug)]
pub struct DiskTrackCache {
    cache_dir: PathBuf,
}

impl DiskTrackCache {
    pub fn new(cache_base_dir: &Path, source_path: &Path) -> std::io::Result<Self> {
        let metadata = fs::metadata(source_path)?;
        let modified = metadata.modified()?;
        let key = Self::compute_key(source_path, modified);

        let cache_dir = cache_base_dir.join(format!("managed_{:016x}", key));
        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir)?;
        }

        Ok(Self { cache_dir })
    }

    fn compute_key(path: &Path, modified: std::time::SystemTime) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        path.to_string_lossy().hash(&mut hasher);
        modified.hash(&mut hasher);
        hasher.finish()
    }

    /// 将音轨事件写入磁盘
    pub fn write_track(&self, track_index: usize, events: &[MidiEvent]) -> std::io::Result<()> {
        let track_path = self.track_path(track_index);
        let serialized = bincode::serialize(events).map_err(std::io::Error::other)?;
        let compressed = zstd::stream::encode_all(&mut &serialized[..], COMPRESSION_LEVEL)
            .map_err(std::io::Error::other)?;
        let mut file = File::create(&track_path)?;
        file.write_all(&compressed)?;
        file.sync_all()?;
        Ok(())
    }

    /// 从磁盘加载音轨事件
    pub fn read_track(&self, track_index: usize) -> std::io::Result<Vec<MidiEvent>> {
        let track_path = self.track_path(track_index);
        if !track_path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("磁盘缓存中找不到音轨 {}", track_index),
            ));
        }
        let file = File::open(&track_path)?;
        let reader = BufReader::new(file);
        let decompressed = zstd::stream::decode_all(reader).map_err(std::io::Error::other)?;
        let events: Vec<MidiEvent> =
            bincode::deserialize(&decompressed).map_err(std::io::Error::other)?;
        Ok(events)
    }

    /// 检查音轨缓存是否存在
    pub fn has_track(&self, track_index: usize) -> bool {
        self.track_path(track_index).exists()
    }

    fn track_path(&self, track_index: usize) -> PathBuf {
        self.cache_dir
            .join(format!("track_{:04x}.zst", track_index))
    }

    /// 清理此源文件对应的缓存
    pub fn cleanup(&self) -> std::io::Result<()> {
        if self.cache_dir.exists() {
            fs::remove_dir_all(&self.cache_dir)?;
        }
        Ok(())
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}
