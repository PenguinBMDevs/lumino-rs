pub mod loader;
pub mod managed_midi;

use std::fs::File;
use std::path::PathBuf;
use memmap2::Mmap;
use midly::{Smf, MidiMessage, MetaMessage, TrackEventKind};

/// MIDI 事件类型（轻量级表示）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MidiEvent {
    NoteOn { track: usize, tick: u32, channel: u8, key: u8, velocity: u8 },
    NoteOff { track: usize, tick: u32, channel: u8, key: u8, velocity: u8 },
    ControlChange { track: usize, tick: u32, channel: u8, controller: u8, value: u8 },
    ProgramChange { track: usize, tick: u32, channel: u8, program: u8 },
    Tempo { track: usize, tick: u32, tempo: u32 },
    TimeSignature { track: usize, tick: u32, numerator: u8, denominator: u8 },
    KeySignature { track: usize, tick: u32, key: i8, is_major: bool },
    TrackName { track: usize, tick: u32, name: String },
    Other { track: usize, tick: u32, raw: Vec<u8> },
}

impl MidiEvent {
    pub fn tick(&self) -> u32 {
        match self {
            MidiEvent::NoteOn { tick, .. } => *tick,
            MidiEvent::NoteOff { tick, .. } => *tick,
            MidiEvent::ControlChange { tick, .. } => *tick,
            MidiEvent::ProgramChange { tick, .. } => *tick,
            MidiEvent::Tempo { tick, .. } => *tick,
            MidiEvent::TimeSignature { tick, .. } => *tick,
            MidiEvent::KeySignature { tick, .. } => *tick,
            MidiEvent::TrackName { tick, .. } => *tick,
            MidiEvent::Other { tick, .. } => *tick,
        }
    }
}

/// MIDI 事件流（使用内存映射，零拷贝）
///
/// 内存占用：仅操作系统页面缓存，不占用用户空间内存
pub struct MidiEventStream {
    _file: File,
    _mmap: Mmap,
    smf: Smf<'static>,
    track_index: usize,
    event_index: usize,
    current_tick: u32,
}

impl MidiEventStream {
    pub fn from_path(path: &std::path::Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("打开文件失败: {e}"))?;

        let mmap = unsafe {
            Mmap::map(&file).map_err(|e| format!("内存映射失败: {e}"))?
        };

        let smf = Smf::parse(&mmap[..]).map_err(|e| format!("解析MIDI失败: {e}"))?;

        let smf_static: Smf<'static> = unsafe {
            std::mem::transmute(smf)
        };

        Ok(Self {
            _file: file,
            _mmap: mmap,
            smf: smf_static,
            track_index: 0,
            event_index: 0,
            current_tick: 0,
        })
    }

    pub fn track_count(&self) -> usize {
        self.smf.tracks.len()
    }

    pub fn division(&self) -> u16 {
        match self.smf.header.timing {
            midly::Timing::Metrical(ticks) => ticks.as_int(),
            _ => 480,
        }
    }

    pub fn read_track_events(&mut self, track_index: usize) -> Result<Vec<MidiEvent>, String> {
        if track_index >= self.smf.tracks.len() {
            return Err(format!("音轨索引 {} 超出范围", track_index));
        }

        let track = &self.smf.tracks[track_index];
        let mut events = Vec::with_capacity(track.len());
        let mut current_tick = 0u32;

        for event in track {
            current_tick = current_tick.saturating_add(u32::from(event.delta));

            let midi_event = self.parse_event(track_index, current_tick, &event.kind);
            if let Some(evt) = midi_event {
                events.push(evt);
            }
        }

        Ok(events)
    }

    pub fn scan_track_for_stats<F>(&self, track_index: usize, mut callback: F) -> Result<u32, String>
    where
        F: FnMut(bool, u32),
    {
        if track_index >= self.smf.tracks.len() {
            return Err(format!("音轨索引 {} 超出范围", track_index));
        }

        let track = &self.smf.tracks[track_index];
        let mut current_tick = 0u32;
        let mut max_tick = 0u32;

        for event in track {
            current_tick = current_tick.saturating_add(u32::from(event.delta));

            match &event.kind {
                TrackEventKind::Midi { channel: _, message } => {
                    match message {
                        MidiMessage::NoteOn { key: _, vel } => {
                            let is_note_on = vel.as_int() > 0;
                            callback(is_note_on, current_tick);
                        }
                        MidiMessage::NoteOff { .. } => {
                            callback(false, current_tick);
                        }
                        MidiMessage::Controller { .. } |
                        MidiMessage::ProgramChange { .. } => {
                            callback(false, current_tick);
                        }
                        _ => {}
                    }
                }
                TrackEventKind::Meta(meta) => {
                    match meta {
                        MetaMessage::Tempo { .. } |
                        MetaMessage::TimeSignature { .. } |
                        MetaMessage::KeySignature { .. } => {
                            callback(false, current_tick);
                        }
                        MetaMessage::EndOfTrack => {
                            max_tick = current_tick;
                            break;
                        }
                        _ => {}
                    }
                }
                TrackEventKind::SysEx(_) | TrackEventKind::Escape(_) => {}
            }
        }

        Ok(max_tick)
    }

    fn parse_event(&self, track_index: usize, tick: u32, kind: &TrackEventKind) -> Option<MidiEvent> {
        match kind {
            TrackEventKind::Midi { channel, message } => {
                let ch = channel.as_int();
                match message {
                    MidiMessage::NoteOn { key, vel } => Some(MidiEvent::NoteOn {
                        track: track_index,
                        tick,
                        channel: ch,
                        key: key.as_int(),
                        velocity: vel.as_int(),
                    }),
                    MidiMessage::NoteOff { key, vel } => Some(MidiEvent::NoteOff {
                        track: track_index,
                        tick,
                        channel: ch,
                        key: key.as_int(),
                        velocity: vel.as_int(),
                    }),
                    MidiMessage::Controller { controller, value } => Some(MidiEvent::ControlChange {
                        track: track_index,
                        tick,
                        channel: ch,
                        controller: controller.as_int(),
                        value: value.as_int(),
                    }),
                    MidiMessage::ProgramChange { program } => Some(MidiEvent::ProgramChange {
                        track: track_index,
                        tick,
                        channel: ch,
                        program: program.as_int(),
                    }),
                    _ => None,
                }
            }
            TrackEventKind::Meta(meta) => {
                match meta {
                    MetaMessage::Tempo(tempo) => Some(MidiEvent::Tempo {
                        track: track_index,
                        tick,
                        tempo: tempo.as_int(),
                    }),
                    MetaMessage::TimeSignature(num, den, _, _) => Some(MidiEvent::TimeSignature {
                        track: track_index,
                        tick,
                        numerator: *num,
                        denominator: *den,
                    }),
                    MetaMessage::KeySignature(key, is_major) => Some(MidiEvent::KeySignature {
                        track: track_index,
                        tick,
                        key: *key,
                        is_major: *is_major,
                    }),
                    MetaMessage::TrackName(name) => Some(MidiEvent::TrackName {
                        track: track_index,
                        tick,
                        name: String::from_utf8_lossy(name).to_string(),
                    }),
                    MetaMessage::EndOfTrack => None,
                    _ => None,
                }
            }
            TrackEventKind::SysEx(_) | TrackEventKind::Escape(_) => None,
        }
    }

    fn next_event(&mut self) -> Option<Result<MidiEvent, String>> {
        loop {
            if self.track_index >= self.smf.tracks.len() {
                return None;
            }

            let track = &self.smf.tracks[self.track_index];

            if self.event_index >= track.len() {
                self.track_index += 1;
                self.event_index = 0;
                self.current_tick = 0;
                continue;
            }

            let event = &track[self.event_index];
            self.event_index += 1;
            self.current_tick = self.current_tick.saturating_add(u32::from(event.delta));

            let track_idx = self.track_index;
            let tick = self.current_tick;

            if let Some(midi_event) = self.parse_event(track_idx, tick, &event.kind) {
                return Some(Ok(midi_event));
            }
        }
    }
}

impl Iterator for MidiEventStream {
    type Item = Result<MidiEvent, String>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_event()
    }
}

/// 解析全部MIDI事件（使用内存映射，低内存）
pub fn parse_all_midi_events(path: &std::path::Path) -> Result<MidiEventStream, String> {
    MidiEventStream::from_path(path)
}

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

/// MIDI文件信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MidiInfo {
    pub path: PathBuf,
    pub track_count: u16,
    pub total_notes: u64,
    pub duration_ticks: u32,
    pub division: u16,
    pub parse_progress: Option<f64>,
}

impl MidiInfo {
    pub fn from_path(path: PathBuf) -> Result<Self, String> {
        Self::from_path_with_progress(path, None)
    }

    pub fn from_path_with_progress(
        path: PathBuf,
        progress_callback: Option<&dyn Fn(f64)>,
    ) -> Result<Self, String> {
        loader::load_midi_info_with_progress(path, progress_callback)
    }
}

impl std::fmt::Display for MidiInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MIDI文件: {}\n音轨数: {}\n音符事件数: {}\n时长: {} ticks\n分辨率: {}",
            self.path.display(),
            self.track_count,
            self.total_notes,
            self.duration_ticks,
            self.division,
        )
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

/// DMS 文件信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DmsInfo {
    pub path: PathBuf,
    pub song_name: Option<String>,
    pub copyright: Option<String>,
    pub comment: Option<String>,
    pub ppqn: Option<u32>,
    pub track_count: usize,
    pub total_notes: u64,
    pub working_time_sec: Option<u64>,
}

impl std::fmt::Display for DmsInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "DMS 文件: {}", self.path.display())?;
        if let Some(ref name) = self.song_name {
            writeln!(f, "歌曲名称: {}", name)?;
        }
        if let Some(ref copyright) = self.copyright {
            writeln!(f, "版权信息: {}", copyright)?;
        }
        if let Some(ppqn) = self.ppqn {
            writeln!(f, "PPQN: {}", ppqn)?;
        }
        writeln!(f, "轨道数量: {}", self.track_count)?;
        writeln!(f, "音符总数: {}", self.total_notes)?;
        if let Some(time) = self.working_time_sec {
            let mins = time / 60;
            let secs = time % 60;
            writeln!(f, "工作时间: {}分{}秒", mins, secs)?;
        }
        Ok(())
    }
}
