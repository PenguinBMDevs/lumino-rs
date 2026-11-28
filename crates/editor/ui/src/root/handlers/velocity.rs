//! 力度编辑面板消息处理器
//!
//! 处理 VelocityAction 相关的消息（力度拖拽、曲线绘制、Tempo 编辑、CC 控制器切换等）。

use crate::editor::velocity::EditMode;
use crate::editor::velocity::layout::TEMPO_BPM_MIN;
use crate::message::{EditorAction, Message, VelocityAction};
use crate::root::Root;
use crate::root::handlers::MessageHandler;
use lumino_note_core::note::Note;

/// 力度编辑面板消息处理器
pub struct VelocityHandler;

impl VelocityHandler {
    /// 创建一个力度编辑面板消息处理器
    pub fn new() -> Self {
        Self
    }

    /// 处理力度编辑面板动作
    ///
    /// 力度 / 曲线分支改完音符后落到尾部同步播放引擎；其余分支按域搬入私有助手，
    /// 原分支里的「不需要重绘」早退在接线处保留。
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
            VA::ToggleMode => return Self::handle_toggle_mode(root), // 不需要重绘
            VA::CcControllerSelected(cc) => {
                root.editor.velocity_panel.selected_cc = cc;
                root.editor.velocity_panel.edit_mode = crate::editor::velocity::EditMode::Cc(cc);
                tracing::debug!("力度面板: 选择 CC 控制器 {}", cc);
                return; // 不需要重绘
            }
            VA::CcOptionSelected(option) => {
                Self::handle_cc_option_selected(root, option);
                return; // 不需要重绘
            }
            // ── Tempo 编辑动作 ──
            action @ (VA::TempoDragStart(_)
            | VA::TempoDragMove(..)
            | VA::TempoDragEnd
            | VA::TempoAdd(..)
            | VA::TempoDelete(_)) => {
                Self::handle_tempo_action(root, action);
                return;
            }
            // ── 自动化曲线编辑动作 ──
            action @ (VA::AutomationEdit(_)
            | VA::AutomationBatch(_)
            | VA::AutomationDragStart
            | VA::AutomationZoom(_)) => {
                Self::handle_automation_action(root, action);
                return;
            }
            VA::WheelScrolled { delta_x, delta_y } => {
                Self::handle_wheel_scrolled(root, delta_x, delta_y);
                return;
            }
        }

        // 同步播放引擎：力度修改必须实时反映到播放中
        if root.editor.notes_changed() {
            root.update_playback_notes();
            root.editor.clear_notes_changed();
        }
    }

    /// 切换编辑模式（力度 / Bend / CC / Tempo），按音轨是否为 Conductor 决定环形顺序。
    fn handle_toggle_mode(root: &mut Root) {
        let panel = &mut root.editor.velocity_panel;
        // 按 id 判定（拖动排序后 conductor 可能不在首位）
        let is_conductor = root.sidebar.selected_track == 0
            && root
                .sidebar
                .tracks
                .iter()
                .find(|t| t.id == 0)
                .is_some_and(|t| t.is_conductor);
        panel.edit_mode = Self::next_edit_mode(panel.edit_mode, is_conductor, panel.selected_cc);
        tracing::debug!("力度面板: 切换模式为 {:?}", panel.edit_mode);
    }

    /// 选择 Bend 或 CC 选项，并同步面板当前 CC 编号。
    fn handle_cc_option_selected(root: &mut Root, option: crate::editor::velocity::CcOption) {
        use crate::editor::velocity::CcOption;
        match option {
            CcOption::Bend => {
                root.editor.velocity_panel.edit_mode = crate::editor::velocity::EditMode::Bend;
                tracing::debug!("力度面板: 选择 Bend");
            }
            CcOption::Cc(cc) => {
                root.editor.velocity_panel.selected_cc = cc;
                root.editor.velocity_panel.edit_mode = crate::editor::velocity::EditMode::Cc(cc);
                tracing::debug!("力度面板: 选择 CC 控制器 {}", cc);
            }
        }
    }

    /// Tempo 编辑动作（拖拽 / 新建 / 删除速度点）：全部不需要重绘。
    ///
    /// 调用方保证只传入 Tempo 族动作，其余变体由 catch-all 忽略。
    fn handle_tempo_action(root: &mut Root, action: VelocityAction) {
        use crate::message::VelocityAction as VA;

        match action {
            VA::TempoDragStart(idx) => {
                root.editor.push_history();
                tracing::debug!("Tempo: 开始拖拽点 {}", idx);
            }
            VA::TempoDragMove(idx, new_bpm) => {
                let max_bpm = root.editor.velocity_panel.tempo_max_bpm;
                let bpm = Self::clamp_tempo_bpm(new_bpm, max_bpm);
                if root.editor.editor_state.data.set_tempo_point(idx, bpm) {
                    root.update_playback_bpm();
                }
            }
            VA::TempoDragEnd => {
                tracing::debug!("Tempo: 拖拽结束");
            }
            VA::TempoAdd(tick, bpm) => {
                root.editor.push_history();
                let max_bpm = root.editor.velocity_panel.tempo_max_bpm;
                let bpm = Self::clamp_tempo_bpm(bpm, max_bpm);
                root.editor.editor_state.data.add_tempo_point(tick, bpm);
                root.update_playback_bpm();
                tracing::debug!("Tempo: 添加点 tick={} bpm={}", tick, bpm);
            }
            VA::TempoDelete(idx) => {
                root.editor.push_history();
                if root.editor.editor_state.data.remove_tempo_point(idx) {
                    root.update_playback_bpm();
                    tracing::debug!("Tempo: 删除点 {}", idx);
                }
            }
            _ => {}
        }
    }

    /// 自动化曲线编辑动作（应用 / 批量应用 / 拖拽开始 / 垂直缩放）：全部不需要重绘。
    ///
    /// 调用方保证只传入自动化族动作，其余变体由 catch-all 忽略。
    fn handle_automation_action(root: &mut Root, action: VelocityAction) {
        use crate::message::VelocityAction as VA;

        match action {
            VA::AutomationEdit(edit) => {
                root.editor.push_history();
                root.editor.editor_state.data.apply_automation_edit(edit);
                root.update_playback_notes();
                tracing::debug!("自动化面板: 应用编辑");
            }
            VA::AutomationBatch(edits) => {
                for edit in edits {
                    root.editor.editor_state.data.apply_automation_edit(edit);
                }
                root.update_playback_notes();
            }
            VA::AutomationDragStart => {
                root.editor.push_history();
                tracing::debug!("自动化面板: 拖拽开始");
            }
            VA::AutomationZoom(factor) => {
                let panel = &mut root.editor.velocity_panel;
                let max_val = Self::automation_max_value(panel.edit_mode);
                let new_zoom = (panel.value_zoom * factor).clamp(0.01, 8.0);
                panel.value_zoom = new_zoom;
                if let Some(max_val) = max_val {
                    panel.clamp_value_scroll(max_val);
                }
                tracing::debug!("自动化面板: 垂直缩放 {}", panel.value_zoom);
            }
            _ => {}
        }
    }

    /// 双向滚轮：水平分量滚动时间轴，垂直分量滚动自动化曲线。
    ///
    /// 垂直分量仅在 CC / Bend 模式生效，力度 / Tempo 模式保持无操作。
    fn handle_wheel_scrolled(root: &mut Root, delta_x: f32, delta_y: f32) {
        // 水平分量：时间轴滚动（与钢琴卷帘网格一致的自然滚动；支持双向同时滚动）
        if delta_x != 0.0 {
            root.handle_editor_action(EditorAction::Scrolled {
                delta_x,
                delta_y: 0.0,
            });
        }
        // 垂直分量：自动化曲线滚动（仅 CC/Bend 模式生效；Velocity/Tempo 保持无操作）
        if delta_y != 0.0 {
            let panel = &mut root.editor.velocity_panel;
            if let Some(max_val) = Self::automation_max_value(panel.edit_mode) {
                let amount = -delta_y * max_val * 0.05;
                panel.value_scroll += amount;
                panel.clamp_value_scroll(max_val);
                tracing::debug!("自动化面板: 垂直滚动 {}", panel.value_scroll);
            }
        }
    }

    /// 将 BPM 值限制在 Tempo 面板合法绘制范围内。
    ///
    /// 上限必须与 `velocity_panel.tempo_max_bpm`（Tempo 面板 BPM 绘制上限）
    /// 保持一致：绘制刻度线/速度点均按该值做 Y 轴线性映射。若此处沿用
    /// 旧硬编码上限（10000），用户把绘制上限调高后，拖拽/新建的速度点
    /// 仍会被截断在 10000，曲线永远无法到达面板顶部。
    fn clamp_tempo_bpm(bpm: f64, max_bpm: f64) -> f64 {
        bpm.clamp(TEMPO_BPM_MIN, max_bpm)
    }

    /// 应用力度值到指定音符，仅在力度实际变化时标记音符变更
    fn apply_velocity(editor: &mut crate::editor::Editor, note_index: usize, velocity: u8) {
        let data = &mut editor.editor_state.data;
        let track_idx = data.current_track;
        // 2026-08 单一权威源：从 document 读取并更新（track_notes 缓存已删除）
        // NoteEvent 为 Copy：先取值再写回，避免借用冲突
        if let Some(note) = data.current_track_notes().get(note_index) {
            let clamped = velocity.clamp(0, 127);
            if note.velocity != clamped {
                let old_note = *note;
                let mut new_note = old_note;
                new_note.velocity = clamped;
                data.update_note(
                    track_idx,
                    note_index,
                    Note::from_raw(
                        old_note.start_tick as f32,
                        old_note.key as u16,
                        (old_note.end_tick - old_note.start_tick) as f32,
                        clamped,
                        old_note.channel,
                    ),
                );
                // 2026-09 协作修复：力度（音符状态）变更需广播给对端，否则 B 端力度失同步。
                // 以「删除旧 + 添加新」入队并广播（复用已修复通道，覆盖全部字段）。
                data.push_collab_transform_transition(old_note, new_note, track_idx);
                editor.broadcast_pending_collab_transform_sync();
                editor.mark_notes_changed();
                tracing::debug!("力度面板: 音符[{}] 力度更新为 {}", note_index, clamped);
            }
        }
    }

    /// 根据当前编辑模式、是否在 Conductor 音轨以及选中的 CC 计算下一个编辑模式
    fn next_edit_mode(mode: EditMode, is_conductor: bool, selected_cc: u8) -> EditMode {
        match (mode, is_conductor) {
            // 普通音轨：Velocity → Bend → Cc(selected_cc) → Velocity
            (EditMode::Velocity, false) => EditMode::Bend,
            (EditMode::Bend, false) => EditMode::Cc(selected_cc),
            (EditMode::Cc(_), false) => EditMode::Velocity,
            // Conductor 音轨：Tempo → Cc(7) → Cc(selected_cc) → Tempo
            (EditMode::Tempo, true) => EditMode::Cc(7),
            (EditMode::Cc(7), true) => {
                if selected_cc == 7 {
                    EditMode::Tempo
                } else {
                    EditMode::Cc(selected_cc)
                }
            }
            (EditMode::Cc(_), true) => EditMode::Tempo,
            // Velocity → 不该在 Conductor 上出现，安全降级到 Tempo
            (EditMode::Velocity, true) => EditMode::Tempo,
            (EditMode::Bend, true) => EditMode::Tempo,
            (EditMode::Tempo, false) => EditMode::Bend,
        }
    }

    /// 根据编辑模式返回自动化目标的最大值，用于垂直缩放/滚动裁剪
    fn automation_max_value(mode: EditMode) -> Option<f32> {
        match mode {
            EditMode::Bend => {
                Some(lumino_note_core::AutomationTarget::PitchBend.max_value() as f32)
            }
            EditMode::Cc(n) => {
                Some(lumino_note_core::AutomationTarget::CC { controller: n }.max_value() as f32)
            }
            _ => None,
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
