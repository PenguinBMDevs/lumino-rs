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

        let file_size = data.len();
        tracing::debug!("读取了 {} 字节", file_size);

        // 解析MIDI
        let (header, track_iter) = midly::parse(&data)
            .map_err(|e| format!("解析失败: {}", e))?;

        // 流式处理音轨，避免同时存储所有音轨
        let mut total_events = 0usize;
        let mut total_notes = 0usize;
        let mut duration_ticks = 0u64;
        let mut last_progress = 0u8;
        let mut track_count = 0usize;

        for (track_idx, track_result) in track_iter.enumerate() {
            let track = track_result
                .map_err(|e| format!("音轨 {} 解析失败: {}", track_idx, e))?;
            
            track_count = track_idx + 1;
            let mut track_ticks = 0u64;

            // 流式处理音轨事件，避免存储中间数据
            for event_result in track {
                match event_result {
                    Ok(event) => {
                        track_ticks += event.delta.as_int() as u64;

                        // 使用 match 优化分支预测
                        match event.kind {
                            midly::TrackEventKind::Midi {
                                message: midly::MidiMessage::NoteOn { .. },
                                ..
                            } => {
                                total_notes += 1;
                            }
                            _ => {}
                        }
                        
                        total_events += 1;
                    }
                    Err(e) => {
                        tracing::warn!("事件解析失败: {}", e);
                    }
                }
            }

            // 更新最大时长
            if track_ticks > duration_ticks {
                duration_ticks = track_ticks;
            }

            // 计算并报告进度（限制回调频率，每1%更新一次）
            if let Some(callback) = progress_callback {
                let progress = ((track_idx + 1) as f64 * 100.0 / 16.0).min(100.0);
                let progress_byte = progress as u8;
                
                if progress_byte > last_progress {
                    last_progress = progress_byte;
                    callback(progress);
                    tracing::debug!("音轨 {}/{} 完成，进度: {:.1}%", track_idx + 1, track_count, progress);
                }
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
