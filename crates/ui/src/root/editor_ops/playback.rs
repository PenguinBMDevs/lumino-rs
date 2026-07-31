//! 编辑器操作 - 播放管理

use crate::playback::{MidiMessage, MidiTrackEvent, NoteEvent};
use crate::root::Root;
use std::sync::Arc;

impl Root {
    /// 更新播放管理器中的音符数据
    ///
    /// 当前轨从 `editor.notes` 实时读取发送到引擎；
    /// 其他音轨靠引擎直接从 document 流式读取，零额外内存。
    pub fn update_playback_notes(&mut self) {
        let Some(manager) = &mut self.playback.manager else {
            return;
        };

        // 更新 MIDI 文档引用（让引擎直接读 document 事件流）
        if let Some(doc) = &self.midi.document {
            manager.set_document(
                Arc::clone(doc),
                self.editor.editor_state.data.current_track as u16,
            );
        }

        // 当前音轨音符（编辑过的，从 editor.notes 实时送）。
        // 力度过滤现在在 PlaybackEngine 内部统一处理，避免当前轨与其他轨行为不一致。
        //
        // NoteStore 热路径：当 note_store_enabled 时直读 SoA 数据，避免：
        // 1. im::Vector 树遍历开销（16M 音符 ~3.5s → 更快）
        // 2. 读取脏数据 bug（commit_pending_drag 跳过 sync_notes_from_store 后
        //    self.notes 位置已过时，而 NoteStore 始终持有最新位置）
        let editor_data = &self.editor.editor_state.data;
        let current_notes: Vec<NoteEvent> = if editor_data.is_note_store_enabled() {
            let mut notes = Vec::with_capacity(editor_data.note_store.len());
            editor_data.note_store.for_each_ref(|_idx, view| {
                notes.push(NoteEvent {
                    tick: view.tick,
                    channel: view.channel,
                    key: view.key as u8,
                    velocity: view.velocity,
                    length: view.length,
                });
            });
            notes
        } else {
            editor_data
                .notes
                .iter()
                .map(|note| NoteEvent {
                    tick: note.tick,
                    channel: note.channel,
                    key: note.key as u8,
                    velocity: note.velocity,
                    length: note.length,
                })
                .collect()
        };
        manager.set_current_track_notes(current_notes);
        manager.set_velocity_filter_threshold(self.visual.velocity_filter_threshold);

        // 同步 MIDI 控制事件
        // 来源 1：从编辑器的 automation_lanes 中提取当前音轨的编辑后控制事件
        let current_track = self.editor.editor_state.data.current_track as u16;

        let mut midi_events: Vec<MidiTrackEvent> = Vec::new();

        // 扫描当前音轨的所有自动化 lane，生成控制事件
        for lane in &self.editor.editor_state.data.automation_lanes {
            if lane.track != current_track {
                continue;
            }
            match &lane.target {
                lumino_core::automation::AutomationTarget::CC { controller } => {
                    for ev in &lane.events {
                        midi_events.push(MidiTrackEvent {
                            tick: ev.tick as f32,
                            message: MidiMessage::ControlChange {
                                channel: lane.channel,
                                controller: *controller,
                                value: ev.value as u8,
                            },
                        });
                    }
                }
                lumino_core::automation::AutomationTarget::PitchBend => {
                    for ev in &lane.events {
                        // AutomationEvent.value 范围 0-16383，中心 8192
                        let pb_value = (ev.value as f32 - 8192.0) / 8192.0;
                        midi_events.push(MidiTrackEvent {
                            tick: ev.tick as f32,
                            message: MidiMessage::PitchBend {
                                channel: lane.channel,
                                value: pb_value.clamp(-1.0, 1.0),
                            },
                        });
                    }
                }
                _ => {
                    // RPN/NRPN 暂不处理
                }
            }
        }

        // 来源 2：其他音轨的预加载控制事件（来自 load_track_midi_events）
        for events in self.playback.track_midi_events.values() {
            midi_events.extend(events.clone());
        }

        // 来源 3：从 document 中读取当前音轨的 ProgramChange 事件。
        // ProgramChange 不存储在 automation_lanes 中（无对应 variant），
        // 必须直接从 doc.control_events 提取，否则当前音轨无法切换乐器。
        // （其他音轨的 PC 事件由 PlaybackEngine::process_other_tracks 直接读取）
        if let Some(doc) = &self.midi.document {
            for ev in &doc.control_events {
                if ev.kind == 1 && ev.track == current_track {
                    let program = ev.as_program_change();
                    midi_events.push(MidiTrackEvent {
                        tick: ev.tick as f32,
                        message: MidiMessage::ProgramChange {
                            channel: ev.channel,
                            program,
                        },
                    });
                }
            }
        }

        midi_events.sort_by(|a, b| a.tick.total_cmp(&b.tick));

        // 调试：记录当前音轨的各类事件数量
        let cc_count = midi_events
            .iter()
            .filter(|e| matches!(e.message, MidiMessage::ControlChange { .. }))
            .count();
        let pc_count = midi_events
            .iter()
            .filter(|e| matches!(e.message, MidiMessage::ProgramChange { .. }))
            .count();
        tracing::debug!(
            "update_playback_notes: 发送 {} 个 MIDI 事件 ({} CC, {} PC, {} PB, current_track={})",
            midi_events.len(),
            cc_count,
            pc_count,
            midi_events
                .iter()
                .filter(|e| matches!(e.message, MidiMessage::PitchBend { .. }))
                .count(),
            current_track,
        );

        manager.set_midi_events(midi_events);
    }

    /// 将编辑器的 tempo_points 同步到播放管理器
    pub fn update_playback_bpm(&mut self) {
        let Some(manager) = &mut self.playback.manager else {
            return;
        };
        let changes: Vec<crate::playback::TempoChange> = self
            .editor
            .editor_state
            .data
            .tempo_points
            .iter()
            .map(|tp| crate::playback::TempoChange::from_bpm(tp.tick, tp.bpm))
            .collect();
        manager.update_tempo_changes(changes);
    }

    /// 重置播放管理器（加载新文件时调用）
    pub fn reset_playback_manager(&mut self) {
        if self.playback.manager.is_some() {
            tracing::info!("Root: 重置播放管理器（新文件加载）");
            self.playback.manager = None;
        }
    }
}
