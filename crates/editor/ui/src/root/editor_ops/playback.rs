//! 编辑器操作 - 播放管理

use std::sync::Arc;

use crate::playback::{MidiMessage, MidiTrackEvent};
use crate::root::Root;

impl Root {
    /// 更新播放管理器中的音符数据
    ///
    /// 当前轨从 `editor_state.data`（document 单一权威源）实时读取发送到引擎；
    /// 其他音轨靠引擎直接从 document 流式读取，零额外内存。
    pub fn update_playback_notes(&mut self) {
        let Some(manager) = &mut self.playback.manager else {
            return;
        };

        let editor_data = &self.editor.editor_state.data;
        let notes_unchanged = !self.editor.notes_changed()
            && self
                .playback
                .last_synced_track_notes_gen
                .is_some_and(|g| g == editor_data.track_notes_gen)
            && self.playback.last_synced_current_track == editor_data.current_track;

        // 只有音符数据或当前音轨变化时才重新发送 document 与当前轨音符，
        // 避免每次小操作（如力度调整、BPM 变更）都重复 clone document。
        if !notes_unchanged {
            // 更新 MIDI 文档引用（让引擎直接读 document 事件流）
            // 2026-08-06 音频修复：从 EditorData.document 克隆快照（ChunkedList
            // 内部 Arc 块级共享，clone 退化为 O(块数) 指针拷贝），包装为 Arc
            // 发送给播放引擎。引擎在 process_other_tracks 中流式读取其他音轨音符，
            // 当前轨队列在 set_document 内从 document 直接重建——不再经
            // Vec<NoteEvent> 中转，消除编辑后全量克隆当前轨音符的 CPU 内存阻塞
            // （1600W 音符工程每次编辑 ~150MB + 200ms 卡顿的根因）。
            if let Some(doc) = editor_data.document.as_ref() {
                manager.set_document(Arc::new(doc.clone()), editor_data.current_track as u16);
            }

            self.playback.last_synced_track_notes_gen = Some(editor_data.track_notes_gen);
            self.playback.last_synced_current_track = editor_data.current_track;
        }

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
                lumino_note_core::automation::AutomationTarget::CC { controller } => {
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
                lumino_note_core::automation::AutomationTarget::PitchBend => {
                    // 弯音应用：按 tick 密集采样（贝塞尔曲线正确插值）。
                    // 采样不写入 lane（只读计算），避免事件数爆炸污染数据模型。
                    // 相邻同值合并 + 上限保护见 `AutomationLane::sample_curve`。
                    let samples = lane.sample_curve(lumino_note_core::MAX_BEND_SAMPLE_EVENTS);
                    for (tick, value) in samples {
                        // AutomationEvent.value 范围 0-16383，中心 8192
                        let pb_value = (value as f32 - 8192.0) / 8192.0;
                        midi_events.push(MidiTrackEvent {
                            tick: tick as f32,
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
        // 2026-08 单一权威源：从 EditorData.document 读取（不再经 midi_state 的 Arc 视图）。
        if let Some(doc) = self.editor.editor_state.data.document.as_ref() {
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
    ///
    /// 播放管理器是懒创建的（首次播放时 `init_playback_manager`）。当管理器尚未
    /// 初始化时，必须与 `load_tempo_changes` 保持一致——把变更缓存到
    /// `pending_tempo_changes`，由首次播放时消费。否则在"空白工程先设置 BPM 再
    /// 从头播放"的操作序列下，tempo 会被静默丢弃，播放回落默认 120 BPM，且后续
    /// 编辑（拖拽多个控制点）全部失效——表现为"只识别第一个 BPM / 首个为默认值"。
    pub fn update_playback_bpm(&mut self) {
        let changes: Vec<crate::playback::TempoChange> = self
            .editor
            .editor_state
            .data
            .tempo_points
            .iter()
            .map(|tp| crate::playback::TempoChange::from_bpm(tp.tick, tp.bpm))
            .collect();

        if let Some(manager) = &mut self.playback.manager {
            manager.update_tempo_changes(changes);
        } else {
            self.playback.pending_tempo_changes = Some(changes);
        }
    }

    /// 重置播放管理器（加载新文件时调用）
    pub fn reset_playback_manager(&mut self) {
        if self.playback.manager.is_some() {
            tracing::info!("Root: 重置播放管理器（新文件加载）");
            self.playback.manager = None;
        }
    }
}
