use midly::{MetaMessage, MidiMessage, Smf, TrackEventKind};
use ouroboros::self_referencing;

/// MIDI 事件类型（轻量级表示）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MidiEvent {
    NoteOn {
        track: usize,
        tick: u32,
        channel: u8,
        key: u8,
        velocity: u8,
    },
    NoteOff {
        track: usize,
        tick: u32,
        channel: u8,
        key: u8,
        velocity: u8,
    },
    ControlChange {
        track: usize,
        tick: u32,
        channel: u8,
        controller: u8,
        value: u8,
    },
    ProgramChange {
        track: usize,
        tick: u32,
        channel: u8,
        program: u8,
    },
    Tempo {
        track: usize,
        tick: u32,
        tempo: u32,
    },
    TimeSignature {
        track: usize,
        tick: u32,
        numerator: u8,
        denominator: u8,
    },
    KeySignature {
        track: usize,
        tick: u32,
        key: i8,
        is_major: bool,
    },
    TrackName {
        track: usize,
        tick: u32,
        name: String,
    },
    Other {
        track: usize,
        tick: u32,
        raw: Vec<u8>,
    },
}

/// 将 midly 的 TrackEventKind 解析为 MidiEvent
///
/// 这是一个纯函数，不依赖任何结构体状态，可以在任何地方使用
pub fn parse_track_event_kind(
    track_index: usize,
    tick: u32,
    kind: &TrackEventKind,
) -> Option<MidiEvent> {
    use midly::{MetaMessage, MidiMessage};

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
        TrackEventKind::Meta(meta) => match meta {
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
            _ => None,
        },
        TrackEventKind::SysEx(data) => Some(MidiEvent::Other {
            track: track_index,
            tick,
            raw: data.to_vec(),
        }),
        TrackEventKind::Escape(data) => Some(MidiEvent::Other {
            track: track_index,
            tick,
            raw: data.to_vec(),
        }),
    }
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

/// MIDI 事件流（直接读取到内存，避免内存映射导致 SIGBUS）
///
/// 内存占用：由于 MIDI 文件通常很小，直接读入内存以避免安全和跨平台文件锁定问题
///
/// # 安全性
///
/// 使用 `ouroboros` 宏安全地处理自引用结构：
/// - `data_holder` 拥有文件数据的 Vec<u8>
/// - `smf` 引用 `data_holder` 中的数据，生命周期由 `ouroboros` 自动管理
#[self_referencing]
pub struct MidiEventStream {
    /// 文件内容
    data_holder: Vec<u8>,
    /// MIDI 解析结果（引用 data_holder 数据）
    #[borrows(data_holder)]
    #[covariant]
    smf: Smf<'this>,
    /// 当前音轨索引
    track_index: usize,
    /// 当前事件索引
    event_index: usize,
    /// 当前 tick
    current_tick: u32,
}

impl MidiEventStream {
    /// 从文件路径创建 MIDI 事件流
    ///
    /// # 安全性
    ///
    /// 使用 `ouroboros` 宏安全地处理自引用结构，避免 unsafe transmute。
    /// 读取文件到内存，避免 mmap 映射被外部修改引发 SIGBUS。
    ///
    /// # 错误
    ///
    /// - 文件打开读取失败
    /// - MIDI 解析失败
    pub fn from_path(path: &std::path::Path) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| format!("读取文件失败: {e}"))?;

        Self::try_new(
            data,
            |data| Smf::parse(data).map_err(|e| format!("解析MIDI失败: {e}")),
            0,
            0,
            0,
        )
        .map_err(|e| format!("创建MidiEventStream失败: {e}"))
    }

    /// 获取音轨数量
    pub fn track_count(&self) -> usize {
        self.with_smf(|smf| smf.tracks.len())
    }

    /// 获取 MIDI 分辨率（PPQN）
    pub fn division(&self) -> u16 {
        self.with_smf(|smf| match smf.header.timing {
            midly::Timing::Metrical(ticks) => ticks.as_int(),
            _ => crate::midi::constants::DEFAULT_PPQN,
        })
    }

    /// 读取指定音轨的所有事件
    pub fn read_track_events(&mut self, track_index: usize) -> Result<Vec<MidiEvent>, String> {
        self.with_smf(|smf| {
            if track_index >= smf.tracks.len() {
                return Err(format!("音轨索引 {} 超出范围", track_index));
            }

            let track = &smf.tracks[track_index];
            let mut events = Vec::with_capacity(track.len());
            let mut current_tick = 0u32;

            for event in track {
                current_tick = current_tick.saturating_add(u32::from(event.delta));

                let midi_event = Self::parse_event_static(track_index, current_tick, &event.kind);
                if let Some(evt) = midi_event {
                    events.push(evt);
                }
            }

            Ok(events)
        })
    }

    /// 扫描音轨统计信息
    pub fn scan_track_for_stats<F>(
        &self,
        track_index: usize,
        mut callback: F,
    ) -> Result<u32, String>
    where
        F: FnMut(bool, u32),
    {
        self.with_smf(|smf| {
            if track_index >= smf.tracks.len() {
                return Err(format!("音轨索引 {} 超出范围", track_index));
            }

            let track = &smf.tracks[track_index];
            let mut current_tick = 0u32;
            let mut max_tick = 0u32;

            for event in track {
                current_tick = current_tick.saturating_add(u32::from(event.delta));

                match &event.kind {
                    TrackEventKind::Midi {
                        channel: _,
                        message,
                    } => match message {
                        MidiMessage::NoteOn { key: _, vel } => {
                            let is_note_on = vel.as_int() > 0;
                            callback(is_note_on, current_tick);
                        }
                        MidiMessage::NoteOff { .. } => {
                            callback(false, current_tick);
                        }
                        MidiMessage::Controller { .. } | MidiMessage::ProgramChange { .. } => {
                            callback(false, current_tick);
                        }
                        _ => {}
                    },
                    TrackEventKind::Meta(meta) => match meta {
                        MetaMessage::Tempo { .. }
                        | MetaMessage::TimeSignature { .. }
                        | MetaMessage::KeySignature { .. } => {
                            callback(false, current_tick);
                        }
                        MetaMessage::EndOfTrack => {
                            max_tick = current_tick;
                            break;
                        }
                        _ => {}
                    },
                    TrackEventKind::SysEx(_) | TrackEventKind::Escape(_) => {}
                }
            }

            Ok(max_tick)
        })
    }

    /// 静态解析事件（不依赖 self）
    fn parse_event_static(
        track_index: usize,
        tick: u32,
        kind: &TrackEventKind,
    ) -> Option<MidiEvent> {
        // 使用公共的解析函数，并过滤掉 EndOfTrack
        match kind {
            TrackEventKind::Meta(MetaMessage::EndOfTrack) => None,
            _ => parse_track_event_kind(track_index, tick, kind),
        }
    }

    /// 获取下一个事件
    fn next_event_internal(&mut self) -> Option<Result<MidiEvent, String>> {
        loop {
            let should_continue = self.with(|fields| {
                let smf = &fields.smf;
                let track_index = *fields.track_index;
                let event_index = *fields.event_index;
                let current_tick = *fields.current_tick;

                if track_index >= smf.tracks.len() {
                    return Ok(None);
                }

                let track = &smf.tracks[track_index];

                if event_index >= track.len() {
                    return Ok(Some(true));
                }

                let event = &track[event_index];
                let new_tick = current_tick.saturating_add(u32::from(event.delta));

                if Self::parse_event_static(track_index, new_tick, &event.kind).is_some() {
                    Ok(Some(false))
                } else {
                    Ok(Some(true))
                }
            });

            match should_continue {
                Ok(None) => return None,
                Ok(Some(false)) => {
                    return self.with(|fields| {
                        let smf = &fields.smf;
                        let track_index = *fields.track_index;
                        let event_index = *fields.event_index;
                        let current_tick = *fields.current_tick;

                        if track_index >= smf.tracks.len() {
                            return None;
                        }

                        let track = &smf.tracks[track_index];
                        if event_index == 0 || event_index > track.len() {
                            return None;
                        }

                        let event = &track[event_index - 1];
                        Self::parse_event_static(track_index, current_tick, &event.kind).map(Ok)
                    });
                }
                Ok(Some(true)) => {
                    self.with_mut(|fields| {
                        let smf = &fields.smf;
                        let track_index = *fields.track_index;
                        let event_index = *fields.event_index;

                        if track_index >= smf.tracks.len() {
                            return;
                        }

                        let track = &smf.tracks[track_index];

                        if event_index >= track.len() {
                            *fields.track_index = track_index + 1;
                            *fields.event_index = 0;
                            *fields.current_tick = 0;
                        } else {
                            let event = &track[event_index];
                            *fields.event_index = event_index + 1;
                            *fields.current_tick =
                                fields.current_tick.saturating_add(u32::from(event.delta));
                        }
                    });
                    continue;
                }
                Err(e) => return Some(Err(e)),
            }
        }
    }
}

impl Iterator for MidiEventStream {
    type Item = Result<MidiEvent, String>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_event_internal()
    }
}

/// 解析全部MIDI事件（读取到内存，低内存占用）
pub fn parse_all_midi_events(path: &std::path::Path) -> Result<MidiEventStream, String> {
    MidiEventStream::from_path(path)
}
