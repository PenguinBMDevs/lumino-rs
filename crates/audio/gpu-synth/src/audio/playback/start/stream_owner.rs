//! 音频流 owner 线程。
//!
//! cpal 0.15 的 `Stream` 在 Windows 上是 `!Send`（携带
//! `NotSendSyncAcrossAllPlatforms`），不能在 `Send` 结构上跨线程移动。因此流
//! 必须在线程内部创建、`play()` 并持有到 `stop_flag` 置位；主线程只保留
//! `JoinHandle`。回调数据来自有界队列，取不到块时填静音并计欠载。

use super::{super::*, recycler::CallbackBufferRecycler};

/// stream owner 线程的启动参数（8 项，按职责成组；逐项传参会触发
/// `too_many_arguments`，且调用点读不出分组）。
pub(super) struct StreamOwnerArgs {
    pub device: cpal::Device,
    pub stream_config: cpal::StreamConfig,
    pub stop_flag: Arc<AtomicBool>,
    pub stats: PlaybackStatsReader,
    pub queue_blocks: usize,
    pub recycle_tx: mpsc::SyncSender<Vec<f32>>,
    pub sample_rx: mpsc::Receiver<Vec<f32>>,
    pub ready_tx: mpsc::Sender<Result<(), SynthError>>,
}

/// 启动 stream owner 线程；「流已就绪 / 失败」经 `ready_tx` 回报。
///
/// 顺序不可调整：建流 → `play()` → 回报 `Ok(())` → 持活到停止。`play()` 之前
/// 回报会让主线程在还没出声时就开始渲染队列。
pub(super) fn spawn(args: StreamOwnerArgs) -> std::io::Result<JoinHandle<()>> {
    let StreamOwnerArgs {
        device,
        stream_config,
        stop_flag,
        stats,
        queue_blocks,
        recycle_tx,
        sample_rx,
        ready_tx,
    } = args;
    thread::Builder::new()
        .name("lumino-gpu-synth-stream-owner".into())
        .spawn(move || {
            let err_fn = |e| {
                log_dual!(error, "lumino-gpu-synth playback error: {e}");
            };
            let owner_stats = stats.clone();
            let mut recycler = CallbackBufferRecycler::new(recycle_tx, queue_blocks);
            let mut next_block: Vec<f32> = Vec::new();
            let mut next_pos = 0usize;
            let stream = match device.build_output_stream(
                &stream_config,
                move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                    // REF-LGS-016：这里曾有「纪元变化 → 丢弃队列旧音频」分支，
                    // 已随死代码一起删除（原因见 `flush_epoch` 处的注释）。
                    recycler.flush();
                    stats
                        .last_request_samples
                        .store(data.len() as i64, Ordering::SeqCst);
                    let mut i = 0;
                    while i < data.len() {
                        if next_pos >= next_block.len() {
                            match sample_rx.try_recv() {
                                Ok(b) => {
                                    let previous = std::mem::replace(&mut next_block, b);
                                    recycler.recycle(previous);
                                    next_pos = 0;
                                }
                                Err(_) => {
                                    data[i..].fill(0.0);
                                    // AUD-012：只有播放中取不到块才算欠载；
                                    // 空闲（未播放 / 播放已结束）填静音不是欠载。
                                    if stats.playing() {
                                        stats.underruns.fetch_add(1, Ordering::Relaxed);
                                    }
                                    break;
                                }
                            }
                        } else {
                            let n = (next_block.len() - next_pos).min(data.len() - i);
                            data[i..i + n].copy_from_slice(&next_block[next_pos..next_pos + n]);
                            i += n;
                            next_pos += n;
                        }
                    }
                    stats.samples.fetch_sub(i as i64, Ordering::SeqCst);
                    stats
                        .last_samples_after_read
                        .store(stats.samples.load(Ordering::SeqCst), Ordering::Relaxed);
                },
                err_fn,
                None,
            ) {
                Ok(s) => s,
                Err(e) => {
                    let _ = ready_tx.send(Err(SynthError::Gpu(format!("audio stream: {e}"))));
                    return;
                }
            };
            if let Err(e) = stream.play() {
                let _ = ready_tx.send(Err(SynthError::Gpu(format!("audio stream play: {e}"))));
                return;
            }
            // 通知主线程流已就绪；随后保持 Stream 存活直到停止。
            let _ = ready_tx.send(Ok(()));
            let _stream = stream;
            let mut last_logged_underruns = 0u64;
            let mut last_underrun_log = Instant::now();
            while !stop_flag.load(Ordering::Relaxed) {
                let underruns = owner_stats.underruns.load(Ordering::Relaxed);
                // AUD-012：只有播放中才把空回调当欠载。空闲时把基线跟到当前值，
                // 保证恢复播放后的第一条真欠载仍会打印（而不是被"已记录过"吞掉）。
                if !owner_stats.playing() {
                    last_logged_underruns = underruns;
                } else if underruns > last_logged_underruns
                    && last_underrun_log.elapsed() >= Duration::from_millis(500)
                {
                    log_dual!(warn, "[UNDERRUN] queue empty (total: {underruns})");
                    last_logged_underruns = underruns;
                    last_underrun_log = Instant::now();
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        })
}
