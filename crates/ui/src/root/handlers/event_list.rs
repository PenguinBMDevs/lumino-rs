//! 事件列表编辑操作处理器
//!
//! 消费 `Sidebar` 缓存的 `EventListAction`，将其应用到 `EditorData`，
//! 并处理跳转请求。所有操作先 push history，保证 Undo/Redo 可用。
//!
//! 音符编辑（删除/插入/字段修改）见同级子模块 `notes`。

use crate::root::Root;
use crate::sidebar::event_browser::{
    EditRequest, EventListAction, JumpRequest, SelectedItem, TextEventKind,
};
use lumino_note_core::event::{ScaleType, SegmentShape};

mod notes;

impl Root {
    /// 处理事件列表跳转请求：切换音轨、移动播放指示线并滚动视口。
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
        // 精确滚动：将钢琴卷帘视口滚到目标 tick，保证可见
        self.scroll_editor_to_tick(req.tick);
        tracing::debug!("Root: 事件列表跳转到 tick {}", req.tick);
    }

    /// 将钢琴卷帘水平滚动到指定 tick（左对齐 + clamp 到最大滚动）。
    ///
    /// 视口宽度由渲染层持有，此处退化为左对齐保证可见；目标超出
    /// 文档末尾时自动 clamp 到末尾。
    fn scroll_editor_to_tick(&mut self, tick: u32) {
        let view = &mut self.editor.editor_state.view;
        let target_x = tick as f32 * view.zoom_x;
        let max_scroll = view.total_ticks as f32 * view.zoom_x;
        view.scroll_x = target_x.min(max_scroll).max(0.0);
        view.smooth_scroll.target_x = view.scroll_x;
        view.smooth_scroll.active = false;
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
            SetLyrics { track, tick, text } => data.set_lyrics_event(track, tick, text),
            SetChord { track, tick, text } => data.set_chord_event(track, tick, text),
            SetProgramChange {
                track,
                tick,
                program,
            } => data.set_program_change_event(track, tick, program),
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
                shape,
            } => value
                .parse::<u32>()
                .ok()
                .map(|new_tick| EventListAction::SetAutomation {
                    track: self.sidebar.selected_track as u16,
                    target: current_automation_target(self),
                    tick: new_tick,
                    value: old,
                    shape,
                }),
            AutoValue {
                tick,
                value: _,
                shape,
            } => value
                .parse::<f32>()
                .ok()
                .map(|new_value| EventListAction::SetAutomation {
                    track: self.sidebar.selected_track as u16,
                    target: current_automation_target(self),
                    tick,
                    value: new_value,
                    shape,
                }),
            AutoShape {
                tick,
                value: old_value,
                shape: old_shape,
            } => {
                let shape = if value == "Curve" {
                    match old_shape {
                        // 保留用户已有的贝塞尔控制点；仅当原来是 Step 时给默认曲线
                        SegmentShape::Curve { .. } => old_shape,
                        SegmentShape::Step => SegmentShape::Curve {
                            x1: 0.25,
                            y1: 0.0,
                            x2: 0.75,
                            y2: 1.0,
                        },
                    }
                } else {
                    SegmentShape::Step
                };
                Some(EventListAction::SetAutomation {
                    track: self.sidebar.selected_track as u16,
                    target: current_automation_target(self),
                    tick,
                    value: old_value,
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
