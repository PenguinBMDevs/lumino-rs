//! 撤销历史对话框：生产状态 → egui 模型，以及动作 → `Message` 映射。

use lumino_core::tr;
use lumino_ui_core::state::HistoryKind;
use lumino_ui_egui::chrome::{HistoryModel, HistoryRowModel};

use crate::Message;

impl crate::root::Root {
    /// 撤销历史模型：初始状态 + 每个步骤（最老在上），当前游标行高亮。
    pub(super) fn history_model(&self) -> HistoryModel {
        let state = &self.state.history_dialog;
        let mut rows = Vec::with_capacity(state.steps.len() + 1);
        rows.push(HistoryRowModel {
            index: 0,
            label: tr!("初始状态").to_string(),
            current: state.current == 0,
            future: false,
        });
        for step in &state.steps {
            rows.push(HistoryRowModel {
                index: step.index,
                label: history_label(step.kind).to_string(),
                current: step.index == state.current,
                future: step.index > state.current,
            });
        }
        HistoryModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: tr!("撤销历史").to_string(),
            hint: tr!("点击步骤回到该步完成后的状态；初始状态为 0。").to_string(),
            rows,
            counter: format!("{} / {}", state.current, state.steps.len()),
            close: tr!("关闭").to_string(),
        }
    }

    /// egui 历史对话框动作 → 生产 `Message`。
    pub(crate) fn history_action_message(
        &self,
        action: &lumino_ui_egui::history::HistoryAction,
    ) -> Option<Message> {
        use crate::message::HistoryAction as Prod;
        use lumino_ui_egui::history::HistoryAction as Action;
        Some(Message::History(match action {
            Action::Jump(index) => Prod::Jump(*index),
            Action::Close => Prod::Close,
        }))
    }
}

/// 历史步骤类型 → 本地化文案（与生产视图同一映射）。
fn history_label(kind: HistoryKind) -> &'static str {
    match kind {
        HistoryKind::NoteCreate => tr!("新建音符"),
        HistoryKind::NoteMove => tr!("移动音符"),
        HistoryKind::NoteDelete => tr!("删除音符"),
        HistoryKind::NoteTransform => tr!("变换音符"),
        HistoryKind::VelocityEdit => tr!("调整力度"),
        HistoryKind::AutomationEdit => tr!("自动化编辑"),
        HistoryKind::Recording => tr!("MIDI 录制"),
        HistoryKind::Other => tr!("其他操作"),
    }
}
