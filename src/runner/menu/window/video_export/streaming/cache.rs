//! 缓存路径与进度上报

use std::path::{Path, PathBuf};

use super::ProgressCallback;

pub(crate) fn build_cache_path(midi_path: &Path) -> Result<PathBuf, String> {
    let stem = midi_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("lumino_video_export");
    let pid = std::process::id();
    let cache_name = format!("{stem}_video_export_notes_{pid}.bin");
    let mut path = std::env::temp_dir().join(cache_name);
    // 如果存在同名文件，追加计数器
    let mut counter = 1;
    while path.exists() {
        path = std::env::temp_dir().join(format!("{stem}_video_export_notes_{pid}_{counter}.bin"));
        counter += 1;
    }
    Ok(path)
}

pub(crate) fn send_progress(
    progress: &Option<ProgressCallback>,
    message: String,
    value: f64,
) -> Result<(), String> {
    if let Some(cb) = progress {
        cb(message, value.clamp(0.0, 1.0));
    }
    Ok(())
}
