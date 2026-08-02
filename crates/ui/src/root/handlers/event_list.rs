//! 事件列表编辑操作处理器
//!
//! 消费 `Sidebar` 缓存的 `EventListAction`，将其应用到 `EditorData`，
//! 并处理跳转请求。所有操作先 push history，保证 Undo/Redo 可用。

use crate::root::Root;
use crate::sidebar::event_browser::{
    EditRequest, EventListAction, JumpRequest, NoteRef, SelectedItem, TextEventKind,
};
use lumino_note_core::event::ScaleType;
use lumino_note_core::note::Note;

impl Root {
    /// 处理事件列表跳转请求：切换音轨并移动播放指示线。
    pub(crate) fn handle_event_list_jump(&mut self, req: &JumpRequest) {
        // 切换音轨（若指定且不同）
        if let Some((track, _key)) = req.note {
            let track = track as usize;
            if self.editor.current_track() != track {
                self.editor.switch_to_track(track);
                self.sidebar.selected_track = track;
            }
        }
        // 设置播放指示线位置（若可用）
        if req.tick > 0 {
            self.editor.playback_position = req.tick as f32;
        }
        tracing::debug!("Root: 事件列表跳转到 tick {}", req.tick);
    }

    /// 将待执行的编辑操作应用到 EditorData。
    pub(crate) fn apply_event_list_action(&mut self, action: EventListAction) {
        use EventListAction::*;
        let data = &mut self.editor.editor_state.data;
        match action {
            DeleteSelected => {
                let ticks = self.sidebar.event_browser_state.selected_ticks.clone();
                self.apply_delete_selected(ticks);
            }
            InsertAbove(tick) => self.apply_insert_at(tick),
            InsertBelow(tick) => self.apply_insert_at(tick),
            InsertFirst => {
                let tick = (self.editor.playback_position.max(0.0)) as u32;
                self.apply_insert_at(tick);
            }
            SetTimeSig {
                tick,
                numerator,
                denominator,
            } => data.set_time_sig_event(tick, numerator, denominator),
            SetKeySig { tick, root, scale } => data.set_key_sig_event(tick, root, scale),
            SetMarker { tick, text } => data.set_marker_event(tick, text),
            SetLyrics { tick, text, .. } => data.set_lyrics_event(tick, text),
            SetChord { tick, text, .. } => data.set_chord_event(tick, text),
            SetProgramChange { tick, program, .. } => data.set_program_change_event(tick, program),
            SetAutomation {
                track,
                target,
                tick,
                value,
                shape,
            } => data.set_automation_event(track, target, tick, value, shape),
            SetNoteStart { note, new_tick } => {
                self.apply_note_edit(&note, |n| {
                    n.tick = new_tick as f32;
                });
            }
            SetNoteEnd { note, new_end_tick } => {
                self.apply_note_edit(&note, |n| {
                    n.length = (new_end_tick as f32 - n.tick).max(0.0);
                });
            }
            SetNoteGate { note, gate } => {
                self.apply_note_edit(&note, |n| {
                    n.length = gate.max(0.0);
                });
            }
            SetNoteKey { note, new_key } => {
                self.apply_note_edit(&note, |n| {
                    n.key = new_key as u16;
                });
            }
            SetNoteVelocity { note, new_velocity } => {
                self.apply_note_edit(&note, |n| {
                    n.velocity = new_velocity.min(127);
                });
            }
        }
        // 编辑后刷新侧边栏音符数据
        self.editor.editor_state.data.sync_track_notes();
        self.editor.spatial.note_index_dirty.set(true);
    }

    /// 删除选中的音符（按 tick 匹配当前音轨）。
    fn apply_delete_selected(&mut self, ticks: std::collections::HashSet<u32>) {
        if ticks.is_empty() {
            return;
        }
        let data = &mut self.editor.editor_state.data;
        data.delete_notes_at_ticks(&ticks);
        // 清空选中状态
        self.sidebar.event_browser_state.selected_ticks.clear();
        self.sidebar.event_browser_state.last_clicked_tick = None;
    }

    /// 在指定 tick 插入新音符（C4, 480 tick）。
    fn apply_insert_at(&mut self, tick: u32) {
        let data = &mut self.editor.editor_state.data;
        if data.current_track == 0 {
            tracing::warn!("Root: Conductor 轨道禁止插入音符");
            return;
        }
        let _ = data.insert_note_at_tick(tick as f32);
    }

    /// 通过 NoteRef 定位音符并应用修改。
    ///
    /// NoteRef 的 `id` 是 (tick, key, length, velocity, channel, track) 的哈希，
    /// 通过匹配原始字段定位 `notes` 中的索引，避免修改后索引漂移。
    fn apply_note_edit(&mut self, note_ref: &NoteRef, f: impl Fn(&mut Note)) {
        let target = (note_ref.start_tick as f32, note_ref.key as u16);
        let data = &mut self.editor.editor_state.data;
        let Some(idx) = data.notes.iter().position(|n| (n.tick, n.key) == target) else {
            tracing::warn!(
                "Root: 未找到音符 start_tick={} key={}",
                note_ref.start_tick,
                note_ref.key
            );
            return;
        };
        data.push_history();
        if let Some(note) = data.notes.get_mut(idx) {
            f(note);
        }
        data.mark_track_notes_changed();
    }

    /// 解析 popup 确认值，生成可执行的编辑操作。
    ///
    /// 需要读取 `EditorData` 当前值（如拍号分母、自动化目标范围）的请求在此处理；
    /// 纯数值解析已在 Sidebar 完成，此处只处理剩余类型。
    pub(crate) fn parse_event_list_edit(
        &mut self,
        req: EditRequest,
        value: String,
    ) -> Option<EventListAction> {
        use EditRequest::*;
        let data = &self.editor.editor_state.data;
        match req {
            AutoTick {
                tick: _,
                value: old,
            } => value
                .parse::<u32>()
                .ok()
                .map(|new_tick| EventListAction::SetAutomation {
                    track: self.sidebar.selected_track as u16,
                    target: current_automation_target(self),
                    tick: new_tick,
                    value: old,
                    shape: lumino_note_core::event::SegmentShape::Step,
                }),
            AutoValue { tick, value: _ } => {
                value
                    .parse::<f32>()
                    .ok()
                    .map(|new_value| EventListAction::SetAutomation {
                        track: self.sidebar.selected_track as u16,
                        target: current_automation_target(self),
                        tick,
                        value: new_value,
                        shape: lumino_note_core::event::SegmentShape::Step,
                    })
            }
            AutoShape { tick, shape: _ } => {
                let shape = if value == "Curve" {
                    lumino_note_core::event::SegmentShape::Curve {
                        x1: 0.25,
                        y1: 0.0,
                        x2: 0.75,
                        y2: 1.0,
                    }
                } else {
                    lumino_note_core::event::SegmentShape::Step
                };
                Some(EventListAction::SetAutomation {
                    track: self.sidebar.selected_track as u16,
                    target: current_automation_target(self),
                    tick,
                    value: 0.0,
                    shape,
                })
            }
            TimeSigTick { tick } => parse_pos(&value).map(|new_tick| {
                let (_, num, den) = data
                    .time_signatures
                    .iter()
                    .find(|(t, _, _)| *t == tick)
                    .copied()
                    .unwrap_or((0, 4, 4));
                EventListAction::SetTimeSig {
                    tick: new_tick,
                    numerator: num,
                    denominator: den,
                }
            }),
            TimeSigNumerator { tick } => value.parse::<u8>().ok().map(|num| {
                let (_, _, den) = data
                    .time_signatures
                    .iter()
                    .find(|(t, _, _)| *t == tick)
                    .copied()
                    .unwrap_or((0, 4, 4));
                EventListAction::SetTimeSig {
                    tick,
                    numerator: num,
                    denominator: den,
                }
            }),
            TimeSigDenominator { tick } => value.parse::<u8>().ok().map(|den| {
                let (_, num, _) = data
                    .time_signatures
                    .iter()
                    .find(|(t, _, _)| *t == tick)
                    .copied()
                    .unwrap_or((0, 4, 4));
                EventListAction::SetTimeSig {
                    tick,
                    numerator: num,
                    denominator: den,
                }
            }),
            KeySigTick { tick } => parse_pos(&value).map(|new_tick| {
                let (_, root, scale) = data
                    .key_signatures
                    .iter()
                    .find(|e| e.tick == tick)
                    .map(|e| (0, e.root, e.scale))
                    .unwrap_or((0, 0, ScaleType::Major));
                EventListAction::SetKeySig {
                    tick: new_tick,
                    root,
                    scale,
                }
            }),
            KeySigRoot { tick } => value.parse::<u8>().ok().map(|root| {
                let scale = data
                    .key_signatures
                    .iter()
                    .find(|e| e.tick == tick)
                    .map(|e| e.scale)
                    .unwrap_or(ScaleType::Major);
                EventListAction::SetKeySig {
                    tick,
                    root: root % 12,
                    scale,
                }
            }),
            KeySigScale { tick } => {
                let root = data
                    .key_signatures
                    .iter()
                    .find(|e| e.tick == tick)
                    .map(|e| e.root)
                    .unwrap_or(0);
                let scale = ScaleType::ALL
                    .iter()
                    .copied()
                    .find(|s| scale_name(s) == value)
                    .unwrap_or(ScaleType::Major);
                Some(EventListAction::SetKeySig { tick, root, scale })
            }
            PcTick { tick } => parse_pos(&value).map(|new_tick| {
                let program = data
                    .program_changes
                    .iter()
                    .find(|e| e.tick == tick)
                    .map(|e| e.program)
                    .unwrap_or(0);
                EventListAction::SetProgramChange {
                    track: self.sidebar.selected_track as u16,
                    tick: new_tick,
                    program,
                }
            }),
            PcProgram { tick } => {
                value
                    .parse::<u8>()
                    .ok()
                    .map(|program| EventListAction::SetProgramChange {
                        track: self.sidebar.selected_track as u16,
                        tick,
                        program: program.min(127),
                    })
            }
            TextEventTick { kind, tick } => {
                parse_pos(&value).map(|new_tick| text_action(kind, tick, new_tick, None))
            }
            TextEventText { kind, tick } => Some(text_action(kind, tick, tick, Some(value))),
            _ => None,
        }
    }
}

/// 解析 "小节/小节内tick" 位置字符串（简化：直接取整数 tick）。
fn parse_pos(value: &str) -> Option<u32> {
    if value.contains('/') {
        value.split('/').next().and_then(|s| s.parse::<u32>().ok())
    } else {
        value.parse::<u32>().ok()
    }
}

/// 根据文本事件类型构造操作。
fn text_action(
    kind: TextEventKind,
    _old_tick: u32,
    new_tick: u32,
    text: Option<String>,
) -> EventListAction {
    let text = text.unwrap_or_default();
    match kind {
        TextEventKind::Marker => EventListAction::SetMarker {
            tick: new_tick,
            text,
        },
        TextEventKind::ConductorLyrics => EventListAction::SetLyrics {
            track: 0,
            tick: new_tick,
            text,
        },
        TextEventKind::ConductorChord => EventListAction::SetChord {
            track: 0,
            tick: new_tick,
            text,
        },
        TextEventKind::Lyrics { track } => EventListAction::SetLyrics {
            track,
            tick: new_tick,
            text,
        },
        TextEventKind::Chord { track } => EventListAction::SetChord {
            track,
            tick: new_tick,
            text,
        },
    }
}

/// 获取当前选中的自动化目标（从 selected_item 读取）。
fn current_automation_target(root: &Root) -> lumino_note_core::event::AutomationTarget {
    match &root.sidebar.event_browser_state.selected_item {
        Some(SelectedItem::Automation { target, .. }) => *target,
        _ => lumino_note_core::event::AutomationTarget::Cc(7),
    }
}

/// 调式名称（与 detail.rs 保持一致）。
fn scale_name(scale: &ScaleType) -> &'static str {
    match scale {
        ScaleType::Major => "Major",
        ScaleType::Minor => "Minor",
        ScaleType::Dorian => "Dorian",
        ScaleType::Phrygian => "Phrygian",
        ScaleType::Lydian => "Lydian",
        ScaleType::Mixolydian => "Mixolydian",
        ScaleType::Aeolian => "Aeolian",
        ScaleType::Locrian => "Locrian",
        ScaleType::HarmonicMinor => "HarmonicMinor",
        ScaleType::MelodicMinor => "MelodicMinor",
    }
}
