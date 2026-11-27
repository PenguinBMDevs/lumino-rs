//! 回调线程归还旧采样块的有界暂存。
//!
//! 音频回调里只做 `try_send`；队列满时把块暂存在本地 `VecDeque`（上限
//! `queue_blocks + 1`），下次回调开头 `flush`。溢出块由 stream-owner 线程
//! 在退出时随队列一起释放，回调线程不做分配。

use super::super::*;

/// 将旧采样块交还渲染线程；回调暂存数量有界，溢出块由 stream-owner 线程释放。
pub(super) struct CallbackBufferRecycler {
    tx: mpsc::SyncSender<Vec<f32>>,
    /// 测试会直接检查暂存内容（`start` 及其子模块可见）。
    pub(super) deferred: std::collections::VecDeque<Vec<f32>>,
    max_deferred: usize,
}

impl CallbackBufferRecycler {
    pub(super) fn new(tx: mpsc::SyncSender<Vec<f32>>, queue_blocks: usize) -> Self {
        Self {
            tx,
            deferred: std::collections::VecDeque::with_capacity(queue_blocks + 1),
            max_deferred: queue_blocks + 1,
        }
    }

    pub(super) fn recycle(&mut self, buffer: Vec<f32>) {
        if let Err(e) = self.tx.try_send(buffer) {
            let buffer = match e {
                mpsc::TrySendError::Full(buffer) | mpsc::TrySendError::Disconnected(buffer) => {
                    buffer
                }
            };
            if self.deferred.len() < self.max_deferred {
                self.deferred.push_back(buffer);
            }
        }
    }

    pub(super) fn flush(&mut self) {
        for _ in 0..self.deferred.len() {
            let Some(buffer) = self.deferred.pop_front() else {
                break;
            };
            if let Err(e) = self.tx.try_send(buffer) {
                let buffer = match e {
                    mpsc::TrySendError::Full(buffer) | mpsc::TrySendError::Disconnected(buffer) => {
                        buffer
                    }
                };
                self.deferred.push_back(buffer);
                break;
            }
        }
    }
}
