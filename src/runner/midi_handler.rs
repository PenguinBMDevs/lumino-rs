use super::midi_parser::{
    parse_midi_events_to_control_events, parse_midi_events_to_notes, TrackMidiEvents,
};
use lumino_core::ParsedMidi;

/// MIDI 处理器
pub struct MidiHandler {
    // MIDI 处理相关状态（如果有）
}

impl MidiHandler {
    pub fn new() -> Self {
        Self {}
    }

    /// 将 MIDI 数据导入到编辑器
    pub fn import_midi_to_editor(
        &self,
        ui: &mut lumino_ui::Host,
        parsed: &ParsedMidi,
    ) {
        use lumino_core::MidiEvent;

        if let Some(memory_manager_arc) = parsed.memory_manager.as_ref() {
            let mut memory_manager: std::sync::MutexGuard<lumino_core::MidiMemoryManager> =
                match memory_manager_arc.lock() {
                    Ok(mgr) => mgr,
                    Err(e) => {
                        tracing::error!("无法锁定 memory_manager: {}", e);
                        return;
                    }
                };

            let mut track_infos = Vec::new();
            let summaries = memory_manager.all_summaries().to_vec();

            for summary in &summaries {
                let track_idx = summary.track_index;

                let track_name: Option<String> =
                    match memory_manager.get_track_events_full(track_idx) {
                        Ok(events) => {
                            events.iter().find_map(|e: &MidiEvent| {
                                if let MidiEvent::TrackName { name, .. } = e {
                                    Some(name.clone())
                                } else {
                                    None
                                }
                            })
                        }
                        Err(e) => {
                            tracing::warn!("无法获取音轨 {} 事件: {}", track_idx, e);
                            None
                        }
                    };

                track_infos.push((track_idx, track_name, summary.note_count));
            }

            ui.set_ppq(parsed.info.division);
            ui.update_tracks(&track_infos);

            self.load_tempo_changes_from_memory_manager(&mut memory_manager, ui);

            tracing::info!("Pre-loading all tracks for onion skin...");
            for (track_idx, _, note_count) in &track_infos {
                if *note_count > 0 {
                    self.preload_track_for_onion_skin(&mut memory_manager, *track_idx, ui);
                }
            }

            if let Some((first_track_idx, _, _)) = track_infos
                .iter()
                .find(|(_, _, note_count)| *note_count > 0)
            {
                self.load_track_to_editor(&mut memory_manager, *first_track_idx, ui);
            }
        } else if let Some(midi_data) = parsed.midi_data.as_ref() {
            tracing::info!("从 midi_data 解析音符数据");
            self.import_midi_data_to_editor(midi_data, parsed.info.track_count as usize, ui);
        } else {
            tracing::warn!("MIDI 没有 memory_manager 也没有 midi_data，无法导入音符");
            return;
        }

        let total_ticks = parsed.info.duration_ticks as f32;
        ui.set_total_ticks(total_ticks);
    }

    /// 从 MIDI 字节流导入音符到编辑器
    pub fn import_midi_data_to_editor(
        &self,
        midi_data: &[u8],
        _track_count: usize,
        ui: &mut lumino_ui::Host,
    ) {
        use super::midi_parser::parse_smf;
        use midly::Smf;

        let smf = match Smf::parse(midi_data) {
            Ok(smf) => smf,
            Err(e) => {
                tracing::error!("解析 MIDI 数据失败: {}", e);
                return;
            }
        };

        let ppq = match smf.header.timing {
            midly::Timing::Metrical(ppq) => ppq.as_int(),
            _ => 1920,
        };
        ui.set_ppq(ppq);

        let (track_infos, track_notes_map, track_events_map) = parse_smf(&smf);

        ui.update_tracks(&track_infos);

        for (track_idx, notes) in track_notes_map {
            ui.load_track_notes(track_idx, &notes);
            tracing::info!("从 midi_data 导入音轨 {}，共 {} 个音符", track_idx, notes.len());
        }

        for (track_idx, events) in track_events_map {
            let ui_events = track_midi_events_to_ui_events(events);
            if !ui_events.is_empty() {
                ui.load_track_midi_events(track_idx, ui_events);
            }
        }

        if let Some((first_track_idx, _, _)) = track_infos
            .iter()
            .find(|(_, _, note_count)| *note_count > 0)
        {
            ui.set_current_track(*first_track_idx);
        }
    }

    /// 加载指定音轨的音符到编辑器
    pub fn load_track_to_editor(
        &self,
        memory_manager: &mut lumino_core::MidiMemoryManager,
        track_idx: usize,
        ui: &mut lumino_ui::Host,
    ) {
        use lumino_core::MidiEvent;

        tracing::info!("load_track_to_editor: track_idx={}", track_idx);

        let events = match memory_manager.get_track_events_full(track_idx) {
            Ok(events) => {
                tracing::info!("  got {} events from track {}", events.len(), track_idx);
                let note_on_count = events
                    .iter()
                    .filter(|e| matches!(e, MidiEvent::NoteOn { .. }))
                    .count();
                let note_off_count = events
                    .iter()
                    .filter(|e| matches!(e, MidiEvent::NoteOff { .. }))
                    .count();
                let cc_count = events
                    .iter()
                    .filter(|e| matches!(e, MidiEvent::ControlChange { .. }))
                    .count();
                let pc_count = events
                    .iter()
                    .filter(|e| matches!(e, MidiEvent::ProgramChange { .. }))
                    .count();
                let pb_count = events
                    .iter()
                    .filter(|e| matches!(e, MidiEvent::PitchBend { .. }))
                    .count();
                tracing::info!(
                    "  NoteOn: {}, NoteOff: {}, CC: {}, PC: {}, PB: {}",
                    note_on_count,
                    note_off_count,
                    cc_count,
                    pc_count,
                    pb_count
                );
                events
            }
            Err(e) => {
                tracing::error!("加载音轨 {} 失败: {}", track_idx, e);
                return;
            }
        };

        let notes = parse_midi_events_to_notes(&events);
        let midi_events = parse_midi_events_to_control_events(&events);

        ui.load_track_notes(track_idx, &notes);
        let ui_events = track_midi_events_to_ui_events(midi_events);
        if !ui_events.is_empty() {
            ui.load_track_midi_events(track_idx, ui_events);
        }

        tracing::info!("音轨 {} 已加载，共 {} 个音符", track_idx, notes.len());
    }

    /// 预加载音轨音符到 track_notes（用于洋葱皮，不切换到该音轨）
    pub fn preload_track_for_onion_skin(
        &self,
        memory_manager: &mut lumino_core::MidiMemoryManager,
        track_idx: usize,
        ui: &mut lumino_ui::Host,
    ) {
        tracing::debug!("Preloading track {} for onion skin", track_idx);

        let events = match memory_manager.get_track_events_full(track_idx) {
            Ok(events) => events,
            Err(e) => {
                tracing::warn!("预加载音轨 {} 失败: {}", track_idx, e);
                return;
            }
        };

        let notes = parse_midi_events_to_notes(&events);
        let midi_events = parse_midi_events_to_control_events(&events);

        if !notes.is_empty() {
            ui.load_track_notes_for_onion_skin(track_idx, &notes);
            tracing::debug!(
                "Preloaded track {} with {} notes for onion skin",
                track_idx,
                notes.len()
            );
        }
        let ui_events = track_midi_events_to_ui_events(midi_events);
        if !ui_events.is_empty() {
            ui.load_track_midi_events_for_onion_skin(track_idx, ui_events);
        }
    }

    /// 从 memory_manager 提取并加载 Tempo 变化事件
    fn load_tempo_changes_from_memory_manager(
        &self,
        memory_manager: &mut lumino_core::MidiMemoryManager,
        ui: &mut lumino_ui::Host,
    ) {
        use lumino_core::MidiEvent;

        let mut tempo_changes = Vec::new();

        let summaries = memory_manager.all_summaries().to_vec();
        for summary in &summaries {
            let track_idx = summary.track_index;

            if let Ok(events) = memory_manager.get_track_events_full(track_idx) {
                for event in events {
                    if let MidiEvent::Tempo { tick, tempo, .. } = event {
                        tempo_changes.push((tick, tempo));
                    }
                }
            }
        }

        tempo_changes.sort_by_key(|(tick, _)| *tick);

        if tempo_changes.is_empty() {
            tempo_changes.push((0, 500000));
            tracing::info!("No tempo events found, using default 120 BPM");
        } else {
            tracing::info!(
                "Loaded {} tempo changes from MIDI file",
                tempo_changes.len()
            );
        }

        ui.load_tempo_changes(tempo_changes);
    }
}

/// 将 TrackMidiEvents 转换为 Vec<MidiTrackEvent>（供 UI 播放引擎使用）
fn track_midi_events_to_ui_events(
    events: TrackMidiEvents,
) -> Vec<lumino_ui::playback::MidiTrackEvent> {
    use lumino_ui::playback::{MidiMessage, MidiTrackEvent};
    let mut result = Vec::new();

    for (tick, channel, controller, value) in events.control_changes {
        result.push(MidiTrackEvent {
            tick,
            message: MidiMessage::ControlChange {
                channel,
                controller,
                value,
            },
        });
    }

    for (tick, channel, program) in events.program_changes {
        result.push(MidiTrackEvent {
            tick,
            message: MidiMessage::ProgramChange { channel, program },
        });
    }

    for (tick, channel, value) in events.pitch_bends {
        result.push(MidiTrackEvent {
            tick,
            message: MidiMessage::PitchBend { channel, value },
        });
    }

    result.sort_by(|a, b| a.tick.total_cmp(&b.tick));
    result
}
