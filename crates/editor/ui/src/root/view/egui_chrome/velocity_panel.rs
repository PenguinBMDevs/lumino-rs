//! 力度 / CC / Tempo 面板：生产状态 → egui 骨架模型。

mod automation;

use lumino_extras::i18n::main_translations;
use lumino_ui_editor::velocity::EditMode;
use lumino_ui_editor::velocity::VelocityPanel;
use lumino_ui_editor::velocity::widget::{
    VelocityCanvas, bend_value_to_y, generate_tempo_levels, tempo_bpm_to_y,
};
use lumino_ui_egui::velocity_panel::{
    LineStrength, TOOLBAR_HEIGHT, VelocityPanelAction, VelocityPanelModel, VelocityPointModel,
    VelocityView,
};

use crate::Message;
use crate::message::VelocityAction;
use crate::root::Root;
use crate::titlebar::mode_toggle::AppMode;

/// 水平滚动条高度（面板位于其下）。
const H_SCROLLBAR: f32 = 20.0;

impl Root {
    /// 本帧力度面板模型；面板不可见时返回 `None`。
    pub fn velocity_panel_model(&self) -> Option<VelocityPanelModel> {
        let sidebar = &self.sidebar;
        if !sidebar.automation_panel_visible
            || !sidebar.piano_roll_visible
            || sidebar.is_arrangement_route()
            || sidebar.audio_export_visible
            || sidebar.video_export_visible
            || self.state.current_mode == AppMode::Waterfall
            || self.is_renderer_entry_active()
        {
            return None;
        }
        let es = &self.editor.editor_state;
        let canvas = &es.canvas;
        let panel = egui::Rect::from_min_size(
            egui::pos2(
                canvas.offset_x,
                canvas.offset_y + canvas.size_y + H_SCROLLBAR,
            ),
            egui::vec2(canvas.size_x, self.visual.velocity_panel_height),
        );
        if panel.width() <= 0.0 || panel.height() <= TOOLBAR_HEIGHT + 1.0 {
            return None;
        }
        let toolbar = egui::Rect::from_min_size(
            panel.min,
            egui::vec2(panel.width(), TOOLBAR_HEIGHT.min(panel.height())),
        );
        let canvas_rect =
            egui::Rect::from_min_max(egui::pos2(panel.left(), toolbar.bottom()), panel.max);

        let vp = &self.editor.velocity_panel;
        let mode = vp.edit_mode;
        let language = self.settings.display.language;
        let t = main_translations(language);
        let is_tempo = mode == EditMode::Tempo;
        let is_velocity = mode == EditMode::Velocity;
        let mode_label = if is_tempo {
            t.velocity_panel_tempo.to_owned()
        } else if is_velocity {
            t.velocity_panel_velocity.to_owned()
        } else if mode == EditMode::Bend {
            "Bend".to_owned()
        } else {
            "CC".to_owned()
        };
        let cc_mode = mode.is_cc();
        let cc_selected = if mode == EditMode::Bend {
            lumino_note_core::BendDisplay.to_string()
        } else {
            lumino_note_core::CcDisplay(vp.selected_cc).to_string()
        };
        let info = if is_tempo {
            t.velocity_panel_tempo_info.to_owned()
        } else if is_velocity {
            t.velocity_panel_velocity_info.to_owned()
        } else if mode == EditMode::Bend {
            "Bend: -8192..8191".to_owned()
        } else {
            lumino_note_core::CcDisplay(vp.selected_cc).to_string()
        };

        let h = canvas_rect.height();
        let hlines: Vec<(f32, String)> = if is_tempo {
            generate_tempo_levels(vp.tempo_max_bpm)
                .into_iter()
                .map(|bpm| {
                    (
                        tempo_bpm_to_y(bpm, vp.tempo_max_bpm, h),
                        format!("{bpm:.0}"),
                    )
                })
                .collect()
        } else if mode == EditMode::Bend {
            [
                (-8192.0, "-8k"),
                (-4096.0, "-4k"),
                (0.0, "0"),
                (4096.0, "+4k"),
                (8191.0, "+8k"),
            ]
            .into_iter()
            .map(|(v, label)| (bend_value_to_y(v as i16, h), label.to_owned()))
            .collect()
        } else {
            [0u8, 32, 64, 96, 127]
                .into_iter()
                .map(|v| (VelocityCanvas::velocity_to_y(v, h), v.to_string()))
                .collect()
        };

        let view = &es.view;
        let ppq = view.ppq as f32;
        let tpm = ppq * 4.0;
        let vis_start = view.scroll_x / view.zoom_x;
        let vis_end = (view.scroll_x + canvas_rect.width() - view.keyboard_width) / view.zoom_x;
        let mut vlines = Vec::new();
        if view.zoom_x > 0.0 && vis_end > vis_start {
            let mut push = |tick: f32, strength| {
                let x = tick * view.zoom_x - view.scroll_x + view.keyboard_width;
                if x >= 0.0 && x <= canvas_rect.width() {
                    vlines.push((x, strength));
                }
            };
            let m0 = (vis_start / tpm).floor() as u32;
            let m1 = (vis_end / tpm).ceil() as u32;
            for m in m0..=m1.min(m0 + 4096) {
                push(m as f32 * tpm, LineStrength::Measure);
            }
            let b0 = (vis_start / ppq).floor() as u32;
            let b1 = (vis_end / ppq).ceil() as u32;
            for b in b0..=b1.min(b0 + 16384) {
                let tick = b as f32 * ppq;
                if (tick % tpm).abs() > f32::EPSILON {
                    push(tick, LineStrength::Beat);
                }
            }
            if view.zoom_x > 0.05 {
                let hb = ppq / 2.0;
                let h0 = (vis_start / hb).floor() as u32;
                let h1 = (vis_end / hb).ceil() as u32;
                for i in h0..=h1.min(h0 + 32768) {
                    let tick = i as f32 * hb;
                    if (tick % tpm).abs() > f32::EPSILON && (tick % ppq).abs() > f32::EPSILON {
                        push(tick, LineStrength::HalfBeat);
                    }
                }
            }
        }

        let velocity_editing =
            is_velocity && self.editor.current_tool() == lumino_message::Tool::Curve;
        let velocity_points = if velocity_editing {
            VelocityPanel::build_velocity_points(es.data.current_track_notes())
                .into_iter()
                .map(|point| VelocityPointModel {
                    note_index: point.note_index,
                    tick: point.tick,
                    velocity: point.velocity,
                })
                .collect()
        } else {
            Vec::new()
        };
        let has_selection = self.editor.has_selection();
        let selected_notes = if has_selection {
            es.interaction.selected_notes.iter().collect()
        } else {
            Vec::new()
        };
        let tempo_editing = is_tempo
            && matches!(
                self.editor.current_tool(),
                lumino_message::Tool::Curve
                    | lumino_message::Tool::Eraser
                    | lumino_message::Tool::DrawEraser
            );
        let tempo_points = if is_tempo {
            VelocityPanel::build_tempo_points(&self.editor)
                .into_iter()
                .map(|point| lumino_ui_egui::velocity_panel::TempoPointModel {
                    tick: point.tick,
                    bpm: point.bpm,
                })
                .collect()
        } else {
            Vec::new()
        };
        let eraser_tool = matches!(
            self.editor.current_tool(),
            lumino_message::Tool::Eraser | lumino_message::Tool::DrawEraser
        );
        let curve_tool = self.editor.current_tool() == lumino_message::Tool::Curve;
        let automation_editing =
            (mode == EditMode::Bend || mode.is_cc()) && (curve_tool || eraser_tool);
        let automation = if mode == EditMode::Bend || mode.is_cc() {
            automation::lane_model(&es.data, mode == EditMode::Bend, vp.selected_cc)
        } else {
            None
        };

        let model = VelocityPanelModel {
            panel,
            toolbar,
            canvas: canvas_rect,
            mode_label,
            mode_primary: is_tempo || is_velocity,
            cc_mode,
            cc_selected,
            info,
            hlines,
            vlines,
            velocity_editing,
            velocity_points,
            has_selection,
            selected_notes,
            view: VelocityView {
                zoom_x: view.zoom_x,
                scroll_x: view.scroll_x,
                keyboard_width: view.keyboard_width,
                snap_precision: view.snap_precision,
            },
            tempo_editing,
            tempo_mode: is_tempo,
            tempo_points,
            tempo_max_bpm: vp.tempo_max_bpm,
            tempo_line_width: vp.automation_line_thickness,
            eraser_tool,
            automation_editing,
            automation,
            value_zoom: vp.value_zoom,
            value_scroll: vp.value_scroll,
        };
        if velocity_editing && std::env::var_os("LUMINO_EGUI_DEBUG_RECTS").is_some_and(|v| v != "0")
        {
            let screen: Vec<(f32, f32)> = model
                .velocity_points
                .iter()
                .map(|p| {
                    (
                        model.canvas.left() + p.tick * model.view.zoom_x - model.view.scroll_x
                            + model.view.keyboard_width,
                        model.canvas.top()
                            + VelocityCanvas::velocity_to_y(p.velocity, model.canvas.height()),
                    )
                })
                .collect();
            tracing::info!(?screen, mode = %model.mode_label, "DEBUG velocity points");
        }
        if tempo_editing && std::env::var_os("LUMINO_EGUI_DEBUG_RECTS").is_some_and(|v| v != "0") {
            let screen: Vec<(f32, f32)> = model
                .tempo_points
                .iter()
                .map(|p| {
                    (
                        model.canvas.left() + p.tick * model.view.zoom_x - model.view.scroll_x
                            + model.view.keyboard_width,
                        model.canvas.top()
                            + tempo_bpm_to_y(p.bpm, model.tempo_max_bpm, model.canvas.height()),
                    )
                })
                .collect();
            tracing::info!(?screen, "DEBUG tempo points");
        }
        if automation_editing
            && std::env::var_os("LUMINO_EGUI_DEBUG_RECTS").is_some_and(|v| v != "0")
        {
            let anchors: Vec<(f32, f32)> = model
                .automation
                .as_ref()
                .map(|lane| {
                    lane.anchors
                        .iter()
                        .map(|a| {
                            (
                                model.canvas.left() + a.tick as f32 * model.view.zoom_x
                                    - model.view.scroll_x
                                    + model.view.keyboard_width,
                                model.canvas.top()
                                    + (model.canvas.height()
                                        - (a.value as f32 / lane.max_value)
                                            * model.canvas.height()),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            tracing::info!(
                ?anchors,
                lane_idx = ?model.automation.as_ref().and_then(|l| l.lane_idx),
                controller = ?model.automation.as_ref().and_then(|l| l.controller),
                canvas = ?model.canvas,
                "DEBUG automation"
            );
        }
        Some(model)
    }

    /// egui 力度面板动作 → 生产消息。
    pub fn velocity_panel_message(&self, action: VelocityPanelAction) -> Message {
        match action {
            VelocityPanelAction::ToggleMode => Message::Velocity(VelocityAction::ToggleMode),
            VelocityPanelAction::CcSelected(cc) => {
                Message::Velocity(VelocityAction::CcOptionSelected(cc))
            }
            VelocityPanelAction::VelocityDragStart(index, velocity) => {
                Message::Velocity(VelocityAction::DragStart(index, velocity))
            }
            VelocityPanelAction::VelocityDragMove(index, velocity) => {
                Message::Velocity(VelocityAction::DragMove(index, velocity))
            }
            VelocityPanelAction::VelocityDragEnd => Message::Velocity(VelocityAction::DragEnd),
            VelocityPanelAction::VelocityCurveStart => {
                Message::Velocity(VelocityAction::CurveStart)
            }
            VelocityPanelAction::VelocityCurvePaint(updates) => {
                Message::Velocity(VelocityAction::CurvePaint(updates))
            }
            VelocityPanelAction::VelocityCurveEnd => Message::Velocity(VelocityAction::CurveEnd),
            VelocityPanelAction::TempoDragStart(index) => {
                Message::Velocity(VelocityAction::TempoDragStart(index))
            }
            VelocityPanelAction::TempoDragMove(index, bpm) => {
                Message::Velocity(VelocityAction::TempoDragMove(index, bpm))
            }
            VelocityPanelAction::TempoDragEnd => Message::Velocity(VelocityAction::TempoDragEnd),
            VelocityPanelAction::TempoAdd(tick, bpm) => {
                Message::Velocity(VelocityAction::TempoAdd(tick, bpm))
            }
            VelocityPanelAction::TempoDelete(index) => {
                Message::Velocity(VelocityAction::TempoDelete(index))
            }
            VelocityPanelAction::AutomationMove {
                track_idx,
                lane_idx,
                old_tick,
                old_value,
                new_tick,
                new_value,
                first,
            } => {
                let edit = lumino_note_core::AutomationEdit::Move {
                    track_idx,
                    lane_idx,
                    old_tick,
                    old_value,
                    new_tick,
                    new_value,
                };
                if first {
                    Message::Velocity(VelocityAction::AutomationEdit(edit))
                } else {
                    Message::Velocity(VelocityAction::AutomationBatch(vec![edit]))
                }
            }
            VelocityPanelAction::AutomationAdd {
                track_idx,
                controller,
                tick,
                value,
            } => {
                let target = match controller {
                    Some(controller) => lumino_note_core::AutomationTarget::CC { controller },
                    None => lumino_note_core::AutomationTarget::PitchBend,
                };
                Message::Velocity(VelocityAction::AutomationEdit(
                    lumino_note_core::AutomationEdit::Add {
                        track_idx,
                        target,
                        channel: 0,
                        tick,
                        value,
                        shape: lumino_note_core::SegmentShape::default(),
                    },
                ))
            }
            VelocityPanelAction::AutomationDelete {
                track_idx,
                lane_idx,
                tick,
            } => Message::Velocity(VelocityAction::AutomationEdit(
                lumino_note_core::AutomationEdit::Delete {
                    track_idx,
                    lane_idx,
                    tick,
                },
            )),
            VelocityPanelAction::Wheel { delta_x, delta_y } => {
                Message::Velocity(VelocityAction::WheelScrolled { delta_x, delta_y })
            }
        }
    }
}
