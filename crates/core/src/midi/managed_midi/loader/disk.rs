use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;

use crate::midi::MidiEvent;

pub fn spawn_disk_writer(
    disk_cache_dir: PathBuf,
    disk_rx: mpsc::Receiver<(usize, Vec<MidiEvent>)>,
) -> std::thread::JoinHandle<Result<(), String>> {
    std::thread::spawn(move || -> Result<(), String> {
        for (track_idx, events) in disk_rx {
            let track_path = disk_cache_dir.join(format!("track_{:04x}.zst", track_idx));
            let serialized = bincode::serialize(&events).map_err(|e| format!("序列化失败: {e}"))?;
            let compressed = zstd::stream::encode_all(&mut &serialized[..], 3)
                .map_err(|e| format!("压缩失败: {e}"))?;
            let mut file_out =
                File::create(&track_path).map_err(|e| format!("创建缓存文件失败: {e}"))?;
            file_out
                .write_all(&compressed)
                .map_err(|e| format!("写入缓存失败: {e}"))?;
        }
        Ok(())
    })
}
