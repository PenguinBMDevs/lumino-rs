//! 音轨持久化状态（可见性 / 独奏）
//!
//! ## 为什么保留这两个字段
//!
//! `visibility` / `solo` 是 `.lmpj` 的**持久化落点**：
//! `LuminoProject::to_midi_document` 在加载工程时写入，
//! `LuminoProject::from_midi_document` 在保存工程时读取
//! （见 `crates/core/project/src/project/conversion.rs`）。UI 侧改动经
//! `Root::apply_track_visibility_to_document` 写回这里。
//!
//! 注意它**不是**：
//! - 音轨名的权威 —— 权威是 `MidiDocument.track_names`（读取一律走 `track_name()`）；
//! - 播放静音的权威 —— 权威是播放引擎自己的 `track_muted` / `track_soloed`
//!   （`PlaybackEngine::track_should_play`）。
//!
//! ## 2026-10-17 死副本清理（审计 §四）
//!
//! 删除全库零调用的成员：`TrackView.track_id`（即索引，冗余）、
//! `TrackView.name`（与 `track_names` 重复且零读取）、
//! `TrackView::{with_name, should_play, should_render}`、
//! `TrackManager::{get_mut, has_solo, playable_tracks, count}`。
//! 其中「是否应播放」的判定在播放引擎侧有活的实现，这里那套只是同名影子。

/// 音轨可见性
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrackVisibility {
    /// 可见（播放/渲染）
    #[default]
    Visible,
    /// 静音（不播放，但仍可渲染）
    Muted,
    /// 隐藏（不播放也不渲染）。
    ///
    /// 仅存在于 `.lmpj` 元数据，UI 未建模（`sidebar::Track` 只有静音/独奏两位），
    /// 因此任何来自 UI 的写回都**不得**把它降级为 `Visible`/`Muted`。
    Hidden,
}

/// 单条音轨的持久化状态（按 `TrackManager` 内位置对应音轨索引）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TrackView {
    /// 当前可见性
    pub visibility: TrackVisibility,
    /// 是否独奏
    pub solo: bool,
}

impl TrackView {
    /// 创建默认状态（可见、非独奏）
    pub fn new() -> Self {
        Self::default()
    }
}

/// 每轨持久化状态表（长度 == 音轨数，按索引对应）
#[derive(Debug, Clone, Default)]
pub struct TrackManager {
    tracks: Vec<TrackView>,
}

impl TrackManager {
    /// 创建 n 条默认状态的音轨
    pub fn new(track_count: u16) -> Self {
        Self {
            tracks: (0..track_count).map(|_| TrackView::new()).collect(),
        }
    }

    /// 追加一条音轨状态（图片转 MIDI 自动建轨用）
    pub fn push(&mut self, track: TrackView) {
        self.tracks.push(track);
    }

    /// 获取音轨状态
    pub fn get(&self, track_id: u16) -> Option<&TrackView> {
        self.tracks.get(track_id as usize)
    }

    /// 设置音轨可见性（越界忽略）
    pub fn set_visibility(&mut self, track_id: u16, visibility: TrackVisibility) {
        if let Some(track) = self.tracks.get_mut(track_id as usize) {
            track.visibility = visibility;
        }
    }

    /// 设置音轨独奏（越界忽略）
    pub fn set_solo(&mut self, track_id: u16, solo: bool) {
        if let Some(track) = self.tracks.get_mut(track_id as usize) {
            track.solo = solo;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_visibility_and_solo_are_per_track() {
        let mut mgr = TrackManager::new(3);
        mgr.set_visibility(1, TrackVisibility::Muted);
        mgr.set_solo(2, true);

        assert_eq!(
            mgr.get(1).map(|v| v.visibility),
            Some(TrackVisibility::Muted)
        );
        assert_eq!(mgr.get(2).map(|v| v.solo), Some(true));
        assert_eq!(
            mgr.get(0).map(|v| v.visibility),
            Some(TrackVisibility::Visible),
            "不得串号到其它音轨"
        );
        assert_eq!(
            mgr.get(0).map(|v| v.solo),
            Some(false),
            "不得串号到其它音轨"
        );
        assert_eq!(
            mgr.get(2).map(|v| v.visibility),
            Some(TrackVisibility::Visible),
            "独奏不应改写可见性"
        );
    }

    #[test]
    fn test_out_of_range_write_is_ignored() {
        let mut mgr = TrackManager::new(1);
        mgr.set_visibility(9, TrackVisibility::Hidden);
        mgr.set_solo(9, true);
        assert_eq!(mgr.get(9).map(|v| v.visibility), None);
    }

    #[test]
    fn test_push_extends_state_table() {
        let mut mgr = TrackManager::new(1);
        mgr.push(TrackView::new());
        mgr.set_visibility(1, TrackVisibility::Muted);
        assert_eq!(
            mgr.get(1).map(|v| v.visibility),
            Some(TrackVisibility::Muted)
        );
    }
}
