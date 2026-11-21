//! 播放管理器的命令队列同步
//!
//! 播放线程是命令的唯一消费者：`Play` / `Pause` / `Stop` / `Seek` 只被投递到
//! 命令通道，真正的状态写入发生在播放线程。调用方（尤其是测试）若在投递后
//! 立即读取 [`PlaybackManager::state`]，读到的可能是尚未执行的旧状态。
//!
//! 本模块提供 FIFO 屏障：投递一个空操作命令并等待回执，回执到达即证明此前
//! 排队的命令已全部执行完毕，从而把「等一会儿再断言」换成「确定已经发生」。

use std::sync::mpsc;
use std::time::Duration;

use super::{PlaybackManager, commands::Command};

/// 屏障回执超时：仅用于兜底「播放线程已退出/卡死」，不参与正确性判断。
///
/// 正常路径下回执在播放线程下一轮排空命令时立即发出（空闲分支阻塞唤醒
/// 即时返回，播放分支迭代约 1ms）；取值远大于调度抖动即可。
const FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

impl PlaybackManager {
    /// 等待播放线程处理完此前排队的全部命令（命令队列 FIFO 屏障）。
    ///
    /// 命令通道是 FIFO 的：屏障回执到达即证明此前发送的 `Play` / `Pause` /
    /// `Stop` / `Seek` 等命令都已执行，此后读取 [`Self::state`]、
    /// [`Self::current_tick`] 等由播放线程写入的状态是确定的。
    ///
    /// 这是「投递命令后立刻断言异步状态」的唯一正确同步方式。用固定次数的
    /// 轮询代替它，会把断言建立在墙钟预算上——并行测试把 CPU 打满时，
    /// 播放线程可能超过预算才被调度，产生偶发失败（曾导致
    /// `test_full_playback_lifecycle` 在整包测试中偶发报「停止后仍为 Playing」）。
    ///
    /// 返回 `false` 表示播放线程已退出，或未在 [`FLUSH_TIMEOUT`] 内回执。
    pub fn flush_commands(&mut self) -> bool {
        let (ack_tx, ack_rx) = mpsc::channel();
        if self.sender.send(Command::Barrier(ack_tx)).is_err() {
            return false;
        }
        ack_rx.recv_timeout(FLUSH_TIMEOUT).is_ok()
    }
}
