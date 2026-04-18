use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::TrackBasedCache;
use crate::cache_utils::compute_cache_key;
use crate::event_cache::TrackEvents;

pub(crate) struct CompressionTask {
    pub(crate) track_idx: usize,
    pub(crate) track_events: Vec<crate::midi::MidiEvent>,
    pub(crate) cache_dir: PathBuf,
    pub(crate) source_path: PathBuf,
}

/// 计算缓存文件路径
pub(crate) fn compute_cache_track_path(
    source_path: &Path,
    cache_dir: &Path,
    track_idx: usize,
) -> std::io::Result<PathBuf> {
    let metadata = std::fs::metadata(source_path)?;
    let modified = metadata.modified()?;
    let key = compute_cache_key(source_path, modified);

    Ok(cache_dir
        .join(format!("{:016x}", key))
        .join(format!("track_{:04x}.lmt", track_idx)))
}

/// 序列化和压缩音轨事件
pub(crate) fn serialize_and_compress_track_events(
    track_events: &[crate::midi::MidiEvent],
    track_idx: usize,
) -> std::io::Result<Vec<u8>> {
    let track_events = TrackEvents {
        track_index: track_idx,
        events: track_events.to_vec(),
    };

    let serialized = bincode::serialize(&track_events).map_err(std::io::Error::other)?;
    let compressed =
        zstd::stream::encode_all(&mut &serialized[..], 3).map_err(std::io::Error::other)?;

    Ok(compressed)
}

pub(crate) fn compression_worker(rx: mpsc::Receiver<CompressionTask>) {
    while let Ok(task) = rx.recv() {
        let result = (|| -> std::io::Result<()> {
            let track_path =
                compute_cache_track_path(&task.source_path, &task.cache_dir, task.track_idx)?;

            let compressed =
                serialize_and_compress_track_events(&task.track_events, task.track_idx)?;

            std::fs::write(&track_path, &compressed)?;

            Ok(())
        })();

        if let Err(e) = result {
            tracing::warn!("缓存音轨 {} 失败: {}", task.track_idx, e);
        }
    }
}

/// 准备缓存目录
pub(crate) fn prepare_cache_dir(path: &Path, cache: Option<&TrackBasedCache>) -> PathBuf {
    if let Some(cc) = cache {
        let cache_dir = cc.cache_dir().to_path_buf();

        if let Ok(metadata) = std::fs::metadata(path)
            && let Ok(modified) = metadata.modified()
        {
            let key = compute_cache_key(path, modified);
            let cache_subdir = cache_dir.join(format!("{:016x}", key));
            if !cache_subdir.exists() {
                let _ = std::fs::create_dir_all(&cache_subdir);
            }
        }
        cache_dir
    } else {
        PathBuf::new()
    }
}
