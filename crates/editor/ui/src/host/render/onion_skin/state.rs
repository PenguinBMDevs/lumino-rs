//! 洋葱皮状态缓存 — 指纹比较、降级节流、CPU 单轨开关。
//!
//! 本模块只持有**标量状态**（不持有 Host 引用），避免借用冲突：
//! 每帧由调用方 `collect_fingerprint` 收集当前指纹后交给 `decide_action`。

use super::DEGRADE_RETRY_INTERVAL;
use super::fingerprint::OnionSkinFingerprint;

/// 洋葱皮状态缓存
///
/// 跟踪 track_notes_gen、音轨开关、当前音轨、调色板变化，避免每帧重建全量实例。
/// 字段以值传递（不持有 Host 引用），避免借用冲突。
///
/// `last_current_track` 默认 `usize::MAX`（哨兵值），确保首次 `needs_rebuild`
/// 即使 `current_track == 0` 也会触发重建（`0 != usize::MAX`）。
/// `last_palette_idx` 默认 `u8::MAX`（哨兵值），确保首次或调色板未初始化时触发重建。
pub(crate) struct OnionSkinState {
    /// 上次构建时的 track_notes_gen（用于检测音符数据变化）
    pub(super) last_track_notes_gen: u64,
    /// 上次构建时的音轨开关指纹（is_muted hash）
    pub(super) last_mute_fingerprint: u64,
    /// 上次构建时的当前音轨（切换主音轨时需重建）
    pub(super) last_current_track: usize,
    /// 上次构建时的调色板索引（切换调色板时需重建）
    pub(super) last_palette_idx: u8,
    /// 上次同步到渲染侧的文档轨数（段表长度；轨数变化 → `TrackLayout` 增量同步）
    pub(super) last_track_count: usize,
    /// 是否已初始化（首次上传）
    pub(super) initialized: bool,
    /// 强制全量重建标志（未知变化兜底：undo/redo/散改/加载后主音轨段
    /// 无事件可对账 → 全量会话分块重建，CPU 峰值可控）
    pub(super) force_full: bool,
    /// CPU 模式：GPU 洋葱皮 buffer 只驻留当前轨（Q3 B）
    ///
    /// 为 `true` 时决策只产出 `Full` / `None`，全量会话只发当前轨段；
    /// 模式切换本身通过 `force_full` 强制下一次全量重建段表。
    pub(super) cpu_current_only: bool,
    /// 降级重试节流截止时间（REND-019 PR-A：通道满降级后的重试风暴防护）
    pub(super) degrade_retry_at: Option<std::time::Instant>,
    /// 降级告警节流截止时间（REND-019 PR-A：日志聚合，避免刷屏）
    pub(super) degrade_warn_at: Option<std::time::Instant>,
}

impl Default for OnionSkinState {
    fn default() -> Self {
        Self {
            last_track_notes_gen: 0,
            last_mute_fingerprint: 0,
            last_current_track: usize::MAX,
            last_palette_idx: u8::MAX,
            last_track_count: 0,
            initialized: false,
            force_full: false,
            cpu_current_only: false,
            degrade_retry_at: None,
            degrade_warn_at: None,
        }
    }
}

impl OnionSkinState {
    /// 标记下一次决策强制全量重建（未知变化兜底，见 `force_full` 字段）
    pub(crate) fn force_full_next(&mut self) {
        self.force_full = true;
    }

    /// 设置 CPU 模式开关。
    ///
    /// 模式变化必须强制下一次全量会话：段表内容从「全轨」变「单轨」或反之，
    /// 增量消息（`TrackDelta` / 段内事件）在该语义下不再成立。
    /// 同一取值重复设置不触发重建（每帧同步调用，避免每帧 Full）。
    pub(crate) fn set_cpu_current_only(&mut self, only_current: bool) {
        if self.cpu_current_only == only_current {
            return;
        }
        self.cpu_current_only = only_current;
        self.force_full = true;
        tracing::debug!(
            "[onion-skin] CPU 单轨模式切换为 {}（强制下一次全量重建）",
            only_current
        );
    }

    /// 降级重试节流是否已就绪（REND-019 PR-A）。
    ///
    /// 渲染线程被 GPU 拖住（TDR 前窗口）时洋葱皮通道会持续满：每次全量
    /// 会话都发不出去，若每帧重试会形成重建风暴（大工程为每帧数百 ms 的
    /// 实例重建）。节流为 ≥ [`DEGRADE_RETRY_INTERVAL`] 重试一次。
    pub(crate) fn degrade_retry_ready(&self) -> bool {
        self.degrade_retry_at
            .is_none_or(|t| std::time::Instant::now() >= t)
    }

    /// 记录一次降级重试开始（重置重试节流窗口）。
    pub(crate) fn note_degrade_retry(&mut self) {
        self.degrade_retry_at = Some(std::time::Instant::now() + DEGRADE_RETRY_INTERVAL);
    }

    /// 降级告警是否到期（`true` 时调用方输出一次 warn，否则降为 debug）。
    pub(crate) fn degrade_warn_due(&mut self) -> bool {
        let now = std::time::Instant::now();
        if self.degrade_warn_at.is_none_or(|t| now >= t) {
            self.degrade_warn_at = Some(now + super::DEGRADE_WARN_INTERVAL);
            true
        } else {
            false
        }
    }

    /// 上次同步到渲染侧的文档轨数（段表长度）
    pub(crate) fn last_track_count(&self) -> usize {
        self.last_track_count
    }

    /// 标记已构建（在重建后调用）
    pub(crate) fn mark_built(&mut self, fp: &OnionSkinFingerprint) {
        self.last_track_notes_gen = fp.track_gen;
        self.last_mute_fingerprint = fp.mute_fp;
        self.last_current_track = fp.current_track;
        self.last_palette_idx = fp.palette_idx;
        self.last_track_count = fp.track_count;
        self.initialized = true;
        self.force_full = false;
    }
}
