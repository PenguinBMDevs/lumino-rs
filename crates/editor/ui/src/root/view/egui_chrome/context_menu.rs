//! 钢琴卷帘右键菜单：生产状态 → egui 模型。
//!
//! 本步把 旧版 `context_menu::view` 的绘制迁到 egui：条目集与文案来自
//! `lumino_message::context_menu`（旧版 侧也已改为同一份表），这里只负责把
//! canvas 局部坐标换算成窗口坐标、并给出可放置区域。

use lumino_message::PianoRollContextMenuAction;
use lumino_ui_egui::piano_roll_context_menu::PianoRollMenuModel;

use crate::Message;
use crate::root::Root;

impl Root {
    /// 当前右键菜单模型；未打开时返回 `None`。
    pub fn piano_roll_context_menu_model(&self) -> Option<PianoRollMenuModel> {
        let menu = &self.editor.context_menu;
        if !menu.open {
            return None;
        }
        let position = menu.position?;
        let canvas = &self.editor.editor_state.canvas;
        let model = PianoRollMenuModel {
            // canvas 局部坐标 → 窗口逻辑坐标
            position: egui::pos2(position.x + canvas.offset_x, position.y + canvas.offset_y),
            available: egui::Rect::from_min_size(
                egui::pos2(canvas.offset_x, canvas.offset_y),
                egui::vec2(canvas.size_x, canvas.size_y),
            ),
            target: menu.target,
        };
        if std::env::var_os("LUMINO_EGUI_DEBUG_RECTS").is_some_and(|v| v != "0") {
            tracing::info!(?model, "DEBUG piano roll context menu");
        }
        Some(model)
    }

    /// egui 菜单动作 → 生产消息（与 旧版 路径同一条消息）。
    pub fn piano_roll_context_menu_message(&self, action: PianoRollContextMenuAction) -> Message {
        Message::PianoRollContextMenu(action)
    }
}
