//! 异步预取线程
//!
//! 独立线程运行，避免播放线程阻塞。
//! 预取当前播放位置前方的 N 块（默认 4），提前加载到 L2 缓存。
//!
//! 设计：
//! - 预取线程通过 channel 接收"当前位置"通知
//! - 独立维护预取队列，避免与播放线程竞争
//! - 预取成功/失败计入 metrics，用于调优

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::cache::LayeredCache;
use crate::params;

/// 预取指令
#[derive(Debug, Clone, Copy)]
pub enum PrefetchCommand {
    /// 跳转到指定 tick
    Seek(u32),
    /// 停止预取
    Stop,
}

/// 预取线程句柄
pub struct PrefetchHandle {
    sender: mpsc::Sender<PrefetchCommand>,
    stopped: Arc<AtomicBool>,
    join_handle: Option<JoinHandle<()>>,
}

impl PrefetchHandle {
    /// 通知预取线程新的播放位置
    pub fn seek(&self, tick: u32) {
        let _ = self.sender.send(PrefetchCommand::Seek(tick));
    }

    /// 停止预取线程
    pub fn stop(&self) {
        let _ = self.sender.send(PrefetchCommand::Stop);
    }

    /// 等待线程结束
    pub fn join(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        let _ = self.sender.send(PrefetchCommand::Stop);
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

/// 启动预取线程
///
/// # 参数
/// - `cache`: L2/L3 缓存的 Arc 引用
/// - `total_ticks`: MIDI 文件总 tick 数
///
/// # 返回值
/// 预取线程句柄，用于发送 seek 指令
pub fn spawn_prefetch_thread(cache: Arc<LayeredCache>, total_ticks: u32) -> PrefetchHandle {
    let (tx, rx) = mpsc::channel::<PrefetchCommand>();
    let stopped = Arc::new(AtomicBool::new(false));
    let stopped_clone = stopped.clone();

    let handle = thread::Builder::new()
        .name("lumino-prefetch".to_string())
        .spawn(move || {
            let mut current_chunk: Option<u32> = None;
            let poll_duration = Duration::from_millis(params::PREFETCH_POLL_MS);

            loop {
                // 检查是否有新指令
                while let Ok(cmd) = rx.try_recv() {
                    match cmd {
                        PrefetchCommand::Seek(tick) => {
                            let chunk_idx = tick / params::CHUNK_TICK_SPAN;
                            current_chunk = Some(chunk_idx);
                        }
                        PrefetchCommand::Stop => {
                            tracing::debug!("Prefetch thread stopping");
                            return;
                        }
                    }
                }

                if stopped_clone.load(Ordering::Relaxed) {
                    return;
                }

                // 如果有当前位置，预取前方块
                if let Some(current) = current_chunk {
                    let max_chunk = total_ticks.div_ceil(params::CHUNK_TICK_SPAN);

                    for i in 1..=params::PREFETCH_AHEAD_COUNT {
                        let target = current.saturating_add(i as u32);
                        if target >= max_chunk {
                            break;
                        }

                        // 尝试预取（如果 L2 已有则跳过）
                        if !cache.prefetch_chunk(target) {
                            break; // L2 满时停止预取
                        }
                    }
                }

                thread::sleep(poll_duration);
            }
        })
        .expect("无法创建预取线程");

    PrefetchHandle {
        sender: tx,
        stopped,
        join_handle: Some(handle),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefetch_command_send() {
        let (tx, rx) = mpsc::channel();
        tx.send(PrefetchCommand::Seek(1000)).unwrap();
        tx.send(PrefetchCommand::Stop).unwrap();

        assert!(matches!(rx.recv(), Ok(PrefetchCommand::Seek(1000))));
        assert!(matches!(rx.recv(), Ok(PrefetchCommand::Stop)));
    }
}
