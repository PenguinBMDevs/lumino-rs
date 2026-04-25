pub mod constants;
pub mod dms;
pub mod event;
pub mod info;
pub mod loader;
pub mod managed_midi;

pub use dms::DmsInfo;
pub use event::{MidiEvent, MidiEventStream, parse_all_midi_events};
pub use info::MidiInfo;

/// LMPJ 文件数据结构（用于序列化/反序列化）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LmpjData {
    pub info: MidiInfo,
    pub midi_data: Option<Vec<u8>>,
}

impl LmpjData {
    pub fn from_parsed_midi(parsed: &ParsedMidi) -> Self {
        Self {
            info: parsed.info.clone(),
            midi_data: parsed.midi_data.clone(),
        }
    }

    pub fn to_parsed_midi(self) -> ParsedMidi {
        ParsedMidi {
            info: self.info,
            midi_data: self.midi_data,
            memory_manager: None,
            cache: None,
        }
    }
}

/// 解析后的MIDI数据
#[derive(Debug, Clone)]
pub struct ParsedMidi {
    pub info: MidiInfo,
    pub midi_data: Option<Vec<u8>>,
    /// 内存管理器（编辑用，按音轨访问）
    pub memory_manager: Option<std::sync::Arc<std::sync::Mutex<managed_midi::MidiMemoryManager>>>,
    /// 分层缓存（播放用，按 tick 跳转）
    pub cache: Option<std::sync::Arc<lumino_cache::MidiCache>>,
}

impl serde::Serialize for ParsedMidi {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(serde::Serialize)]
        struct Helper<'a> {
            info: &'a crate::MidiInfo,
            midi_data: &'a Option<Vec<u8>>,
        }
        Helper {
            info: &self.info,
            midi_data: &self.midi_data,
        }
        .serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for ParsedMidi {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Helper {
            info: crate::MidiInfo,
            midi_data: Option<Vec<u8>>,
        }
        let h = Helper::deserialize(deserializer)?;
        Ok(ParsedMidi {
            info: h.info,
            midi_data: h.midi_data,
            memory_manager: None,
            cache: None,
        })
    }
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

    pub fn build_track_cache(
        &self,
        cache: &crate::TrackBasedCache,
    ) -> Result<crate::TrackCacheHeader, String> {
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
        if !cache
            .has_cache(&self.info.path)
            .map_err(|e| format!("检查缓存失败: {e}"))?
        {
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
            mgr.lock()
                .map(|guard| guard.stats())
                .unwrap_or_else(|poisoned| {
                    let guard = poisoned.into_inner();
                    guard.stats()
                })
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
