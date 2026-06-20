use super::midi_parser::TrackMidiEvents;
use lumino_midi_loader::ParsedMidi;

/// MIDI 处理器
pub struct MidiHandler;

impl MidiHandler {
    pub fn new() -> Self {
        Self
    }

    /// 将 MIDI 数据导入到编辑器
    pub fn import_midi_to_editor(&self, ui: &mut lumino_ui::Host, parsed: &ParsedMidi) {
        ui.reset_playback_manager();

        let Some(document) = parsed.document.as_ref() else {
            // LMPJ 文件加载时已同步构建 MidiDocument，理论上不应走到此路径
            tracing::warn!("MIDI 没有 document，无法导入");
            return;
        };
        let document = document.as_ref();
        tracing::info!("导入 MIDI 文档：{} 音轨", document.track_count());

        let track_count = document.track_count();
        let mut track_infos = Vec::with_capacity(track_count);

        // 只收集音轨信息（名称、音符数），不预加载音符到 track_notes
        // 音符将在首次渲染或切换音轨时从 MidiDocument 懒加载
        // 这样可以避免 track_notes + MidiDocument 两份数据共存导致内存翻倍
        // 注意：使用 track_note_count 而非 get_track_notes 以避免全量提取
        for track_idx in 0..track_count {
            let note_count = document.track_note_count(track_idx as u16);
            let track_name = document.track_name(track_idx).map(|s| s.to_string());
            track_infos.push((track_idx, track_name, note_count));
        }

        ui.set_ppq(parsed.info.division);
        ui.update_tracks(&track_infos);

        // 将 MidiDocument 传递给编辑器供懒加载使用
        // Arc::clone 是 O(1) 引用计数递增，避免深拷贝 120MB+ 事件数据
        ui.set_midi_document(document.clone());

        // 从预存储的 tempo_changes 加载
        let tempo_ui: Vec<(u32, u32)> = document
            .tempo_changes
            .iter()
            .map(|&(tick, bpm)| {
                let microseconds = if bpm > 0.0 {
                    lumino_midi_loader::bpm_to_tempo(bpm as f64)
                } else {
                    lumino_midi_loader::constants::DEFAULT_TEMPO_MICROS
                };
                (tick, microseconds)
            })
            .collect();
        if !tempo_ui.is_empty() {
            ui.load_tempo_changes(tempo_ui);
        }

        // 加载第一个有音符的音轨到编辑器（实际显示 + 懒加载缓存）
        if let Some((first_track_idx, _, _)) = track_infos
            .iter()
            .find(|(_, _, note_count)| *note_count > 0)
        {
            let first_notes = document.get_track_notes(*first_track_idx as u16);
            ui.load_track_notes(*first_track_idx, &first_notes);
            ui.set_current_track(*first_track_idx);
        }

        tracing::info!(
            "加载完成: {} 音轨, {} ticks, 音符已加载",
            track_count,
            document.total_ticks()
        );

        let total_ticks = parsed.info.duration_ticks as f32;
        ui.set_total_ticks(total_ticks);
    }

    /// 从 MIDI 字节流导入音符到编辑器（用于 LMPJ 文件）
    pub fn import_midi_data_to_editor(
        &self,
        midi_data: &[u8],
        _track_count: usize,
        ui: &mut lumino_ui::Host,
    ) {
        ui.reset_playback_manager();
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
            _ => lumino_midi_loader::constants::DEFAULT_PPQN,
        };
        ui.set_ppq(ppq);

        let (track_infos, track_notes_map, track_events_map) = parse_smf(&smf);

        ui.update_tracks(&track_infos);

        for (track_idx, notes) in track_notes_map {
            ui.load_track_notes(track_idx, &notes);
            tracing::info!(
                "从 midi_data 导入音轨 {}，共 {} 个音符",
                track_idx,
                notes.len()
            );
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
