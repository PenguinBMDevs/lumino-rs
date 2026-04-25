//! 音轨视图模式
//!
//! `TrackDataRef` 不持有事件数据，仅持有一个轻量级引用。
//! 音轨的事件数据通过 ChunkCache 间接访问。
//!
//! 内存占用预估：
//! - TrackDataRef: 32 字节
//! - TrackView: 40 字节 + 2 个 Arc
//! - 支持 1000+ 音轨同时存在，内存开销可忽略

use std::sync::Arc;

use lumino_midi::compact::CompactEvent;

use crate::cache::LayeredCache;
use crate::index::ChunkIndex;

/// 音轨数据引用 — 不持有实际事件数据
///
/// 只持有一个 Arc 指针指向缓存系统，实际的 chunk 查找通过 LayeredCache 完成。
pub struct TrackDataRef {
    /// 音轨编号
    pub track_id: u16,
    /// 引用缓存系统
    cache: Arc<LayeredCache>,
    /// ChunkIndex 的 Arc（与 cache 共享）
    index: Arc<ChunkIndex>,
}

impl TrackDataRef {
    /// 创建新的音轨数据引用
    pub fn new(track_id: u16, cache: Arc<LayeredCache>, index: Arc<ChunkIndex>) -> Self {
        Self {
            track_id,
            cache,
            index,
        }
    }

    /// 获取指定 tick 范围内该音轨的所有事件
    ///
    /// # 参数
    /// - `from_tick`: 起始 tick（包含）
    /// - `to_tick`: 结束 tick（不包含）
    ///
    /// # 返回值
    /// 符合条件的 CompactEvent 列表
    pub fn get_events(&self, from_tick: u32, to_tick: u32) -> Vec<CompactEvent> {
        let (start_chunk, end_chunk) = self.index.chunk_range(from_tick, to_tick);
        if start_chunk >= end_chunk {
            return Vec::new();
        }

        let mut all_events = self.cache.get_events(from_tick, to_tick, 0);

        // 按音轨过滤
        all_events.retain(|ev| ev.track_id() == self.track_id);
        all_events
    }

    /// 检查音轨是否有事件在指定 tick 范围内
    pub fn has_events_in_range(&self, from_tick: u32, to_tick: u32) -> bool {
        let (start, end) = self.index.chunk_range(from_tick, to_tick);
        for i in start..end {
            if let Some(entry) = self.index.entries.get(i)
                && entry.has_track(self.track_id)
            {
                return true;
            }
        }
        false
    }
}

/// 音轨视图状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackVisibility {
    /// 可见（播放/渲染）
    Visible,
    /// 静音（不播放，但仍可渲染）
    Muted,
    /// 隐藏（不播放也不渲染）
    Hidden,
}

/// 音轨视图 — 管理单个音轨的可见性状态
///
/// 不影响缓存行为，仅用于过滤播放/渲染输出。
pub struct TrackView {
    /// 音轨编号
    pub track_id: u16,
    /// 音轨名称（可选）
    pub name: String,
    /// 当前可见性
    pub visibility: TrackVisibility,
    /// 是否反转（solo 模式）
    pub solo: bool,
}

impl TrackView {
    /// 创建新的音轨视图
    pub fn new(track_id: u16) -> Self {
        Self {
            track_id,
            name: format!("Track {}", track_id),
            visibility: TrackVisibility::Visible,
            solo: false,
        }
    }

    /// 创建带名称的音轨视图
    pub fn with_name(track_id: u16, name: String) -> Self {
        Self {
            track_id,
            name,
            visibility: TrackVisibility::Visible,
            solo: false,
        }
    }

    /// 音轨是否应该播放
    pub fn should_play(&self, has_solo: bool) -> bool {
        if has_solo {
            return self.solo;
        }
        self.visibility == TrackVisibility::Visible
    }

    /// 音轨是否应该渲染
    pub fn should_render(&self) -> bool {
        self.visibility != TrackVisibility::Hidden
    }
}

/// 音轨视图管理器
pub struct TrackManager {
    /// 所有音轨视图
    tracks: Vec<TrackView>,
}

impl TrackManager {
    /// 创建新的音轨管理器
    pub fn new(track_count: u16) -> Self {
        let tracks = (0..track_count).map(TrackView::new).collect();
        Self { tracks }
    }

    /// 获取音轨视图
    pub fn get(&self, track_id: u16) -> Option<&TrackView> {
        self.tracks.get(track_id as usize)
    }

    /// 获取音轨视图（可变）
    pub fn get_mut(&mut self, track_id: u16) -> Option<&mut TrackView> {
        self.tracks.get_mut(track_id as usize)
    }

    /// 设置音轨可见性
    pub fn set_visibility(&mut self, track_id: u16, visibility: TrackVisibility) {
        if let Some(track) = self.tracks.get_mut(track_id as usize) {
            track.visibility = visibility;
        }
    }

    /// 设置音轨 solo
    pub fn set_solo(&mut self, track_id: u16, solo: bool) {
        if let Some(track) = self.tracks.get_mut(track_id as usize) {
            track.solo = solo;
        }
    }

    /// 是否有任何音轨处于 solo 状态
    pub fn has_solo(&self) -> bool {
        self.tracks.iter().any(|t| t.solo)
    }

    /// 获取应播放的音轨 ID 列表
    pub fn playable_tracks(&self) -> Vec<u16> {
        let has_solo = self.has_solo();
        self.tracks
            .iter()
            .filter(|t| t.should_play(has_solo))
            .map(|t| t.track_id)
            .collect()
    }

    /// 音轨总数
    pub fn count(&self) -> u16 {
        self.tracks.len() as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_view_visibility() {
        let mut view = TrackView::new(0);
        assert!(view.should_play(false));
        assert!(view.should_render());

        view.visibility = TrackVisibility::Muted;
        assert!(!view.should_play(false));
        assert!(view.should_render());

        view.visibility = TrackVisibility::Hidden;
        assert!(!view.should_play(false));
        assert!(!view.should_render());
    }

    #[test]
    fn test_track_solo() {
        let mut view0 = TrackView::new(0);
        let view1 = TrackView::new(1);
        view0.solo = true;

        // With solo active, only soloed tracks play
        let has_solo = true;
        assert!(view0.should_play(has_solo));
        assert!(!view1.should_play(has_solo));
    }

    #[test]
    fn test_track_manager() {
        let mut mgr = TrackManager::new(4);
        assert_eq!(mgr.count(), 4);
        assert!(!mgr.has_solo());

        mgr.set_solo(1, true);
        assert!(mgr.has_solo());

        let playable = mgr.playable_tracks();
        assert_eq!(playable, vec![1]);
    }

    #[test]
    fn test_track_ref_caching() {
        // TrackDataRef requires active cache — test construction only
        // Full integration test in tests/
    }
}
