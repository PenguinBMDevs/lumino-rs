//! 力度编辑面板消息处理器
//!
//! 处理 VelocityAction 相关的消息（力度拖拽、曲线绘制、Tempo 编辑、CC 控制器切换等）。

use crate::editor::editor_state::TempoPoint;
use crate::message::{Message, VelocityAction};
use crate::root::Root;
use crate::root::handlers::MessageHandler;

/// 力度编辑面板消息处理器
pub struct VelocityHandler;

impl VelocityHandler {
    pub fn new() -> Self {
        Self
    }

    /// 处理力度编辑面板动作
    pub fn handle_action(root: &mut Root, action: VelocityAction) {
        use crate::message::VelocityAction as VA;

        match action {
            VA::DragStart(note_index, velocity) => {
                root.editor.push_history();
                Self::apply_velocity(&mut root.editor, note_index, velocity);
            }
            VA::DragMove(note_index, new_velocity) => {
                Self::apply_velocity(&mut root.editor, note_index, new_velocity);
            }
            VA::DragEnd => {
                tracing::debug!("力度面板: 拖拽结束");
            }
            VA::CurveStart => {
                root.editor.push_history();
                tracing::debug!("力度面板: 曲线绘制开始");
            }
            VA::CurvePaint(updates) => {
                for (note_index, velocity) in updates {
                    Self::apply_velocity(&mut root.editor, note_index, velocity);
                }
            }
            VA::CurveEnd => {
                tracing::debug!("力度面板: 曲线绘制结束");
            }
            VA::ToggleMode => {
                let panel = &mut root.editor.velocity_panel;
                let is_conductor = root.sidebar.selected_track == 0
                    && root.sidebar.tracks.first().is_some_and(|t| t.is_conductor);
                panel.edit_mode = match (panel.edit_mode, is_conductor) {
                    // 普通音轨：Velocity → Bend → Cc(selected_cc) → Velocity
                    (crate::editor::velocity::EditMode::Velocity, false) => {
                        crate::editor::velocity::EditMode::Bend
                    }
                    (crate::editor::velocity::EditMode::Bend, false) => {
                        crate::editor::velocity::EditMode::Cc(panel.selected_cc)
                    }
                    (crate::editor::velocity::EditMode::Cc(_), false) => {
                        crate::editor::velocity::EditMode::Velocity
                    }
                    // Conductor 音轨：Tempo → Cc(7) → Cc(selected_cc) → Tempo
                    (crate::editor::velocity::EditMode::Tempo, true) => {
                        crate::editor::velocity::EditMode::Cc(7)
                    }
                    (crate::editor::velocity::EditMode::Cc(7), true) => {
                        if panel.selected_cc == 7 {
                            crate::editor::velocity::EditMode::Tempo
                        } else {
                            crate::editor::velocity::EditMode::Cc(panel.selected_cc)
                        }
                    }
                    (crate::editor::velocity::EditMode::Cc(_), true) => {
                        crate::editor::velocity::EditMode::Tempo
                    }
                    // Velocity → 不该在 Conductor 上出现，安全降级到 Tempo
                    (crate::editor::velocity::EditMode::Velocity, true) => {
                        crate::editor::velocity::EditMode::Tempo
                    }
                    (crate::editor::velocity::EditMode::Bend, true) => {
                        crate::editor::velocity::EditMode::Tempo
                    }
                    (crate::editor::velocity::EditMode::Tempo, false) => {
                        crate::editor::velocity::EditMode::Bend
                    }
                };
                tracing::debug!("力度面板: 切换模式为 {:?}", panel.edit_mode);
                return; // 不需要重绘
            }
            VA::CcControllerSelected(cc) => {
                root.editor.velocity_panel.selected_cc = cc;
                root.editor.velocity_panel.edit_mode = crate::editor::velocity::EditMode::Cc(cc);
                tracing::debug!("力度面板: 选择 CC 控制器 {}", cc);
                return; // 不需要重绘
            }
            VA::CcOptionSelected(option) => {
                use crate::editor::velocity::CcOption;
                match option {
                    CcOption::Bend => {
                        root.editor.velocity_panel.edit_mode =
                            crate::editor::velocity::EditMode::Bend;
                        tracing::debug!("力度面板: 选择 Bend");
                    }
                    CcOption::Cc(cc) => {
                        root.editor.velocity_panel.selected_cc = cc;
                        root.editor.velocity_panel.edit_mode =
                            crate::editor::velocity::EditMode::Cc(cc);
                        tracing::debug!("力度面板: 选择 CC 控制器 {}", cc);
                    }
                }
                return; // 不需要重绘
            }
            // ── Tempo 编辑动作 ──
            VA::TempoDragStart(idx) => {
                root.editor.push_history();
                tracing::debug!("Tempo: 开始拖拽点 {}", idx);
                return;
            }
            VA::TempoDragMove(idx, new_bpm) => {
                let bpm = new_bpm.clamp(20.0, 10000.0);
                if let Some(point) = root.editor.editor_state.data.tempo_points.get_mut(idx) {
                    point.bpm = bpm;
                    root.update_playback_bpm();
                }
                return;
            }
            VA::TempoDragEnd => {
                tracing::debug!("Tempo: 拖拽结束");
                return;
            }
            VA::TempoAdd(tick, bpm) => {
                root.editor.push_history();
                let bpm = bpm.clamp(20.0, 10000.0);
                root.editor
                    .editor_state
                    .data
                    .tempo_points
                    .push(TempoPoint { tick, bpm });
                root.editor.editor_state.data.tempo_points.sort_by(|a, b| {
                    a.tick
                        .partial_cmp(&b.tick)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                // 去重相同 tick
                root.editor
                    .editor_state
                    .data
                    .tempo_points
                    .dedup_by(|a, b| (a.tick - b.tick).abs() < f32::EPSILON);
                root.update_playback_bpm();
                tracing::debug!("Tempo: 添加点 tick={} bpm={}", tick, bpm);
                return;
            }
            VA::TempoDelete(idx) => {
                root.editor.push_history();
                if idx < root.editor.editor_state.data.tempo_points.len() {
                    root.editor.editor_state.data.tempo_points.remove(idx);
                    root.update_playback_bpm();
                    tracing::debug!("Tempo: 删除点 {}", idx);
                }
                return;
            }
        }

        // 同步播放引擎：力度修改必须实时反映到播放中
        if root.editor.notes_changed() {
            root.update_playback_notes();
            root.editor.clear_notes_changed();
            root.invalidate_onion_skin_cache();
        }
    }

    /// 应用力度值到指定音符，仅在力度实际变化时标记音符变更
    fn apply_velocity(editor: &mut crate::editor::Editor, note_index: usize, velocity: u8) {
        let data = &mut editor.editor_state.data;
        if note_index < data.notes.len()
            && let Some(note) = data.notes.get_mut(note_index)
        {
            let clamped = velocity.clamp(0, 127);
            if note.velocity != clamped {
                note.velocity = clamped;
                editor.mark_notes_changed();
                tracing::debug!("力度面板: 音符[{}] 力度更新为 {}", note_index, clamped);
            }
        }
    }
}

impl Default for VelocityHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageHandler for VelocityHandler {
    fn handle(&mut self, root: &mut Root, msg: Message) -> Option<Message> {
        match msg {
            Message::Velocity(action) => {
                Self::handle_action(root, action);
                None
            }
            other => Some(other),
        }
    }
}
