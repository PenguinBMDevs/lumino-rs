//! MIDI文件处理模块
//!
//! 提供异步MIDI文件解析功能，支持实时进度回调

use std::path::PathBuf;

/// MIDI文件信息结构
#[derive(Debug, Clone)]
pub struct MidiInfo {
    pub path: PathBuf,
    pub header: midly::Header,
    pub track_count: usize,
    pub total_events: usize,
    pub total_notes: usize,
    pub duration_ticks: u64,
    pub parse_progress: Option<f64>,
}

impl MidiInfo {
    /// 异步解析MIDI文件
    ///
    /// # Arguments
    /// * `path` - MIDI文件路径
    pub async fn from_path(path: PathBuf) -> Result<Self, String> {
        Self::from_path_with_progress(path, None).await
    }

    /// 异步解析MIDI文件（带进度回调）
    ///
    /// # Arguments
    /// * `path` - MIDI文件路径
    /// * `progress_callback` - 可选的进度回调，接收0.0-100.0的进度值
    pub async fn from_path_with_progress(
        path: PathBuf,
        progress_callback: Option<&dyn Fn(f64)>,
    ) -> Result<Self, String> {
        // 异步读取文件
        let data = tokio::fs::read(&path)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        tracing::debug!("Read {} bytes from {:?}", data.len(), path);

        // 使用midly::parse获取懒惰迭代器
        let (header, track_iter) = midly::parse(&data)
            .map_err(|e| format!("MIDI parse error: {}", e))?;

        // 收集所有track数据
        let tracks: Vec<_> = track_iter
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Track parse error: {}", e))?;

        // 计算总字节数用于进度计算
        let total_track_bytes: usize = tracks.iter()
            .map(|track| track.unread().len())
            .sum();

        let track_count = tracks.len();
        tracing::info!("Processing {} tracks, {} total bytes",
            track_count, total_track_bytes);

        // 遍历所有tracks
        let mut total_events = 0;
        let mut total_notes = 0;
        let mut duration_ticks = 0u64;
        let mut processed_bytes = 0usize;

        for (track_idx, track) in tracks.into_iter().enumerate() {
            let track_bytes = track.unread().len();
            let mut track_ticks = 0u64;

            // 处理这个track的所有事件
            for event in track {
                match event {
                    Ok(ev) => {
                        track_ticks += ev.delta.as_int() as u64;

                        if let midly::TrackEventKind::Midi {
                            message: midly::MidiMessage::NoteOn { .. }, ..
                        } = ev.kind {
                            total_notes += 1;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse event in track {}: {}", track_idx, e);
                    }
                }
                total_events += 1;
            }
            duration_ticks = duration_ticks.max(track_ticks);

            // 更新已处理的字节数
            processed_bytes += track_bytes;

            // 报告进度（每个track完成后报告一次）
            if let Some(callback) = progress_callback {
                let progress = if total_track_bytes > 0 {
                    (processed_bytes as f64 / total_track_bytes as f64) * 100.0
                } else {
                    0.0
                };
                callback(progress);

                tracing::debug!("Track {}/{} parsed, progress: {:.1}%",
                    track_idx + 1, track_count, progress);
            }
        }

        tracing::info!(
            "Parsed MIDI: {} tracks, {} events, {} notes, {} ticks",
            track_count,
            total_events,
            total_notes,
            duration_ticks
        );

        Ok(Self {
            path,
            header,
            track_count,
            total_events,
            total_notes,
            duration_ticks,
            parse_progress: Some(100.0),
        })
    }
}

impl std::fmt::Display for MidiInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let format_str = match self.header.format {
            midly::Format::SingleTrack => "Single Track",
            midly::Format::Parallel => "Parallel",
            midly::Format::Sequential => "Sequential",
        };

        let timing_str = match self.header.timing {
            midly::Timing::Metrical(ticks) => format!("{} ticks/quarter", ticks.as_int()),
            midly::Timing::Timecode(fps, subframe) => {
                format!("{:?} fps, {} subframes", fps, subframe)
            }
        };

        write!(
            f,
            "MIDI File: {}\n\
             Format: {}\n\
             Timing: {}\n\
             Tracks: {}\n\
             Total Events: {}\n\
             Note Events: {}\n\
             Duration: {} ticks",
            self.path.display(),
            format_str,
            timing_str,
            self.track_count,
            self.total_events,
            self.total_notes,
            self.duration_ticks
        )
    }
}
