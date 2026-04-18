use crate::midi::MidiEvent;
use crate::midi::managed_midi::loader::estimate_events_size;

use super::types::MidiMemoryManager;

impl MidiMemoryManager {
    /// 获取音轨事件（编辑/浏览用）
    ///
    /// 如果在内存中，直接返回引用；
    /// 如果在磁盘上，加载到 loaded_tracks 中（受 LRU 管理），再返回引用。
    pub fn get_track_events(&mut self, track_index: usize) -> Result<&[MidiEvent], String> {
        if track_index >= self.track_summaries.len() {
            return Err(format!("音轨索引 {} 超出范围", track_index));
        }

        if self.in_memory_tracks.contains_key(&track_index) {
            return Ok(self
                .in_memory_tracks
                .get(&track_index)
                .ok_or(format!("音轨 {} 内存数据意外丢失", track_index))?);
        }

        if self.loaded_tracks.contains_key(&track_index) {
            self.touch_lru(track_index);
            return Ok(self
                .loaded_tracks
                .get(&track_index)
                .ok_or(format!("音轨 {} 加载数据意外丢失", track_index))?);
        }

        let events = self
            .disk_cache
            .read_track(track_index)
            .map_err(|e| format!("从磁盘加载音轨 {} 失败: {e}", track_index))?;

        let event_size = estimate_events_size(&events);

        while self.loaded_memory_used + event_size > self.loaded_memory_limit
            && !self.lru_order.is_empty()
        {
            self.evict_oldest_loaded();
        }

        self.loaded_memory_used += event_size;
        self.loaded_tracks.insert(track_index, events);
        self.lru_order.push(track_index);
        Ok(self
            .loaded_tracks
            .get(&track_index)
            .ok_or(format!("音轨 {} 插入后数据意外丢失", track_index))?)
    }

    /// 获取音轨事件的完整数据（用于编辑，需要可变访问）
    ///
    /// 所有音轨的完整数据都在磁盘上（包括 InMemory 的音轨）。
    /// InMemory 中只有过滤后的数据，编辑需要完整数据。
    pub fn get_track_events_full(&mut self, track_index: usize) -> Result<Vec<MidiEvent>, String> {
        if track_index >= self.track_summaries.len() {
            return Err(format!("音轨索引 {} 超出范围", track_index));
        }

        if self.disk_cache.has_track(track_index) {
            let events = self
                .disk_cache
                .read_track(track_index)
                .map_err(|e| format!("从磁盘加载音轨 {} 失败: {e}", track_index))?;
            return Ok(events);
        }

        if let Some(events) = self.in_memory_tracks.get(&track_index) {
            return Ok(events.clone());
        }

        Err(format!("音轨 {} 数据不存在", track_index))
    }

    /// 卸载所有按需加载的音轨
    pub fn unload_all_loaded(&mut self) {
        let freed: usize = self
            .loaded_tracks
            .values()
            .map(|e| estimate_events_size(e))
            .sum();
        self.loaded_tracks.clear();
        self.lru_order.clear();
        self.loaded_memory_used = self.loaded_memory_used.saturating_sub(freed);
    }

    /// 卸载指定按需加载的音轨
    pub fn unload_track(&mut self, track_index: usize) {
        if let Some(events) = self.loaded_tracks.remove(&track_index) {
            let size = estimate_events_size(&events);
            self.loaded_memory_used = self.loaded_memory_used.saturating_sub(size);
            self.lru_order.retain(|&i| i != track_index);
        }
    }

    fn touch_lru(&mut self, track_index: usize) {
        if let Some(pos) = self.lru_order.iter().position(|&i| i == track_index) {
            self.lru_order.remove(pos);
        }
        self.lru_order.push(track_index);
    }

    fn evict_oldest_loaded(&mut self) {
        if let Some(oldest) = self.lru_order.first().copied() {
            if let Some(events) = self.loaded_tracks.remove(&oldest) {
                let size = estimate_events_size(&events);
                self.loaded_memory_used = self.loaded_memory_used.saturating_sub(size);
            }
            self.lru_order.remove(0);
        }
    }
}
