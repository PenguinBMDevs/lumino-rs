use std::path::PathBuf;

/// MIDI文件信息
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
    /// 解析MIDI文件
    pub async fn from_path(path: PathBuf) -> Result<Self, String> {
        Self::from_path_with_progress(path, None).await
    }

    /// 解析MIDI文件（带进度回调）
    pub async fn from_path_with_progress(
        path: PathBuf,
        progress_callback: Option<&dyn Fn(f64)>,
    ) -> Result<Self, String> {
        // 读取文件
        let data = tokio::fs::read(&path)
            .await
            .map_err(|e| format!("读取文件失败: {}", e))?;

        tracing::debug!("读取了 {} 字节", data.len());

        // 解析MIDI
        let (header, track_iter) = midly::parse(&data)
            .map_err(|e| format!("解析失败: {}", e))?;

        // 收集所有音轨
        let tracks: Vec<_> = track_iter
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("音轨解析失败: {}", e))?;

        // 计算总字节数
        let total_track_bytes: usize = tracks.iter()
            .map(|track| track.unread().len())
            .sum();

        let track_count = tracks.len();
        tracing::info!("处理 {} 个音轨，共 {} 字节", track_count, total_track_bytes);

        // 统计信息
        let mut total_events = 0;
        let mut total_notes = 0;
        let mut duration_ticks = 0u64;
        let mut processed_bytes = 0usize;

        for (track_idx, track) in tracks.into_iter().enumerate() {
            let track_bytes = track.unread().len();
            let mut track_ticks = 0u64;

            // 处理每个事件
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
                        tracing::warn!("音轨 {} 事件解析失败: {}", track_idx, e);
                    }
                }
                total_events += 1;
            }
            duration_ticks = duration_ticks.max(track_ticks);

            // 更新进度
            processed_bytes += track_bytes;

            if let Some(callback) = progress_callback {
                let progress = if total_track_bytes > 0 {
                    (processed_bytes as f64 / total_track_bytes as f64) * 100.0
                } else {
                    0.0
                };
                callback(progress);

                tracing::debug!("音轨 {}/{} 完成，进度: {:.1}%", track_idx + 1, track_count, progress);
            }
        }

        tracing::info!(
            "解析完成: {} 个音轨, {} 个事件, {} 个音符, {} ticks",
            track_count, total_events, total_notes, duration_ticks
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
            midly::Format::SingleTrack => "单音轨",
            midly::Format::Parallel => "并行",
            midly::Format::Sequential => "顺序",
        };

        let timing_str = match self.header.timing {
            midly::Timing::Metrical(ticks) => format!("{} ticks/四分音符", ticks.as_int()),
            midly::Timing::Timecode(fps, subframe) => {
                format!("{:?} fps, {} 子帧", fps, subframe)
            }
        };

        write!(
            f,
            "MIDI文件: {}\n\
             格式: {}\n\
             时间: {}\n\
             音轨数: {}\n\
             总事件数: {}\n\
             音符事件数: {}\n\
             时长: {} ticks",
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
