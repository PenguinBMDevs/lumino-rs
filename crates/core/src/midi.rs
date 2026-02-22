pub mod loader;
pub mod managed_midi;
pub mod event;
pub mod info;
pub mod dms;

pub use event::{MidiEvent, MidiEventStream, parse_all_midi_events};
pub use info::MidiInfo;
pub use dms::DmsInfo;

/// 解析后的MIDI数据
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParsedMidi {
    pub info: MidiInfo,
    #[serde(skip)]
    pub midi_data: Option<Vec<u8>>,
    /// 内存管理器（新架构）
    #[serde(skip)]
    pub memory_manager: Option<std::sync::Arc<std::sync::Mutex<managed_midi::MidiMemoryManager>>>,
}

impl ParsedMidi {
    pub fn take_midi_data(&mut self) -> Option<Vec<u8>> {
        self.midi_data.take()
    }

    pub fn events_stream(&self) -> Result<MidiEventStream, String> {
        MidiEventStream::from_path(&self.info.path)
    }

    pub fn parse_all_events(&self) -> Result<Vec<MidiEvent>, String> {
        self.events_stream()?.collect()
    }

    pub fn build_track_cache(&self, cache: &crate::TrackBasedCache) -> Result<crate::TrackCacheHeader, String> {
        let mut stream = self.events_stream()?;
        cache
            .build_cache_streaming(&self.info.path, &mut stream)
            .map_err(|e| format!("构建缓存失败: {e}"))
    }

    pub fn open_track_window(
        &self,
        cache: &crate::TrackBasedCache,
        max_loaded_tracks: usize,
    ) -> Result<crate::TrackEventWindow, String> {
        if !cache.has_cache(&self.info.path).map_err(|e| format!("检查缓存失败: {e}"))? {
            self.build_track_cache(cache)?;
        }

        cache
            .open_window(&self.info.path, max_loaded_tracks)
            .map_err(|e| format!("打开事件窗口失败: {e}"))
    }

    /// 使用内存管理的方式获取音轨事件（编辑/浏览用）
    pub fn get_managed_track_events(&self, track_index: usize) -> Result<Vec<MidiEvent>, String> {
        if let Some(mgr) = &self.memory_manager {
            let mut mgr = mgr.lock().map_err(|e| format!("锁定内存管理器失败: {e}"))?;
            let events = mgr.get_track_events(track_index)?;
            Ok(events.to_vec())
        } else {
            // 回退到流式读取
            let mut stream = self.events_stream()?;
            stream.read_track_events(track_index)
        }
    }

    /// 获取内存管理器统计
    pub fn manager_stats(&self) -> Option<managed_midi::ManagerStats> {
        self.memory_manager.as_ref().map(|mgr| {
            mgr.lock().unwrap().stats()
        })
    }
}

/// 解析后的 DMS 数据（轻量级）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParsedDms {
    pub info: DmsInfo,
    #[serde(skip)]
    data: Option<lumino_dms::DmsLightweightData>,
}

impl ParsedDms {
    pub fn parse_full(&self) -> Result<lumino_dms::DmsCompositeNode, String> {
        self.data
            .as_ref()
            .ok_or_else(|| "需要加载完整DMS数据才能解析".to_string())?
            .parse_full()
            .map_err(|e| format!("解析 DMS 节点树失败: {e}"))
    }

    pub fn data_size(&self) -> usize {
        self.data.as_ref().map(|d| d.len()).unwrap_or(0)
    }
}
