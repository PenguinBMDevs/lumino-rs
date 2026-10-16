//! 洋葱皮通道发送（REND-019 PR-A′，规范 §1.6 `send_timeout` 方案）。
//!
//! 历史教训（2026-10-17 release 端到端实测）：`try_send` 立即丢弃使多块全量
//! 会话（BeginSession + Reserve + N×Chunk + Done，大工程 100+ 条消息）必然
//! 残缺、`Done` 常丢——渲染侧以 `BeginSession` 清段表重开、`Done` 才
//! `finish_streaming_upload`，残缺会话永远无法提交 → 音符/洋葱皮整层不渲染。
//! 改回规范允许的 `send_timeout`：正常提供背压（会话必然完整送达），等待有
//! 上限（渲染线程卡死时 UI 不被永久阻塞）。

use std::sync::atomic::Ordering;
use std::time::Duration;

use crossbeam_channel::{SendTimeoutError, Sender};

use super::WgpuRenderThread;

/// 洋葱皮通道发送超时（REND-019 PR-A′，规范 §1.6 `send_timeout` 方案）。
///
/// 正常背压应在一个渲染循环（毫秒级）内被消费；500ms 只在「渲染线程存活但被
/// GPU 调用卡住」（TDR 前窗口 / 驱动挂起）时触发——此时消息丢弃、置降级标志，
/// 交由 UI 侧 `force_full_next` 节流全量补发，UI 线程最多阻塞本超时。
const ONION_SKIN_SEND_TIMEOUT: Duration = Duration::from_millis(500);

/// 洋葱皮消息发送结果（[`send_with_timeout`] 语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OnionSendOutcome {
    /// 已送达
    Sent,
    /// 通道满且等待超时（消息已丢弃；调用方应中止当前会话等待补发）
    Timeout,
    /// 通道断开（渲染线程已退出 / 已 shutdown）
    Disconnected,
}

/// 带超时的有界通道发送（REND-019 PR-A′）。
///
/// 正常提供背压（等渲染线程消费，满时为按序等待），等待上限
/// [`ONION_SKIN_SEND_TIMEOUT`]——渲染线程卡死时 UI 线程不被永久阻塞。
pub(super) fn send_with_timeout<T>(
    sender: &Sender<T>,
    msg: T,
    timeout: Duration,
) -> OnionSendOutcome {
    match sender.send_timeout(msg, timeout) {
        Ok(()) => OnionSendOutcome::Sent,
        Err(SendTimeoutError::Timeout(_)) => OnionSendOutcome::Timeout,
        Err(SendTimeoutError::Disconnected(_)) => OnionSendOutcome::Disconnected,
    }
}

impl WgpuRenderThread {
    /// 发送洋葱皮上传消息（全量会话：Chunk + Done；事件级增量：TrackDelta）
    ///
    /// 全量会话：UI 线程分块构建后发送若干 `Chunk{track_id, instances}`，
    /// 最后发送 `Done`。
    ///
    /// 事件级增量：编辑非当前/非静音音轨时，只发送
    /// `TrackDelta{track_id, instances}`（该音轨段整体替换）。
    ///
    /// REND-019（PR-A′ §1.6，`send_timeout` 方案）：**禁止裸 `send` / `try_send`**——
    /// - 裸 `send`：渲染线程卡死（TDR / GPU 挂起）时满通道 send 会永久阻塞 UI
    ///   线程（死锁兑现路径）；
    /// - 裸 `try_send`：多块全量会话（大工程 100+ 条消息）必然残缺、`Done` 常丢
    ///   → 渲染侧永远收不到完整会话 → 音符/洋葱皮整层不渲染
    ///   （2026-10-17 release 端到端实测教训）。
    ///
    /// 现方案：有界背压 + [`ONION_SKIN_SEND_TIMEOUT`] 上限——
    /// - 正常：等待渲染线程消费（背压），全量会话必然完整送达；
    /// - 线程已退出（alive=false）→ 快速失败；
    /// - 等待超时（线程存活但卡死）→ 丢弃 + 置降级标志（UI 侧 `force_full_next`
    ///   节流全量补发）；
    /// - 通道断开 → 丢弃并告警。
    ///
    /// # 返回值
    /// `true` = 已送达；`false` = 未送达——调用方**必须中止当前会话**
    /// （不再发送后续 Chunk/Done），等待降级补发。
    #[must_use]
    pub fn send_onion_skin_msg(&self, msg: crate::OnionSkinStreamMsg) -> bool {
        if !self.is_alive() {
            tracing::warn!("洋葱皮消息丢弃：渲染线程已退出");
            return false;
        }
        let Some(ref sender) = self.onion_skin_streaming_sender else {
            tracing::warn!("洋葱皮消息丢弃：发送端已释放（已 shutdown）");
            return false;
        };
        match send_with_timeout(sender, msg, ONION_SKIN_SEND_TIMEOUT) {
            OnionSendOutcome::Sent => true,
            OnionSendOutcome::Timeout => {
                self.onion_skin_degraded.store(true, Ordering::SeqCst);
                // 告警由 UI 侧聚合节流输出（持续卡死时逐条 warn 会刷屏）——此处仅记 debug
                tracing::debug!("洋葱皮通道发送超时：消息丢弃并置位降级标志（UI 侧节流补发）");
                false
            }
            OnionSendOutcome::Disconnected => {
                tracing::warn!("洋葱皮消息丢弃：渲染线程通道已断开");
                false
            }
        }
    }
}
