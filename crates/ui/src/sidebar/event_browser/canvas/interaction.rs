//! 事件浏览器 Canvas 交互逻辑。
//!
//! 从 canvas.rs 拆分，保持主文件 < 400 行。
//! 实现 EventBrowserCanvas 的鼠标/键盘事件处理。

use iced_core::keyboard::Key;
use iced_core::keyboard::key::Named;
use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas;

use crate::Message;
use crate::sidebar::Event;

use super::hit_test;
use super::popup::{self, PopupHit};
use super::{CanvasState, EventBrowserCanvas, HEADER_HEIGHT, SPLITTER_WIDTH};
use crate::sidebar::event_browser::detail;
use crate::sidebar::event_browser::edit::PopupState;
use crate::sidebar::event_browser::state::{ArchiveKey, EditRequest, SelectedItem};
use crate::sidebar::event_browser::tree::TreeItem;
impl<'a> EventBrowserCanvas<'a> {
    /// 处理左键按下
    pub(super) fn handle_left_press(
        &self,
        state: &mut CanvasState,
        bounds: Rectangle,
        local: Point,
    ) -> Option<canvas::Action<Message>> {
        // 弹窗打开时：命中弹窗按钮
        if state.popup.is_some() {
            return self.handle_popup_click(state, bounds, local);
        }
        // 上下文菜单打开时：命中菜单项
        if state.context_menu.is_some() {
            return self.handle_context_menu_click(state, bounds, local);
        }

        let tree_w = self.tree_width(bounds, state);
        // 树/表格分隔条
        if (local.x - tree_w - SPLITTER_WIDTH * 0.5).abs() <= SPLITTER_WIDTH * 0.5 {
            state.splitter_dragging = true;
            state.splitter_start_x = local.x;
            state.splitter_start_ratio = state.tree_width;
            return Some(canvas::Action::capture());
        }

        if local.x < tree_w {
            // 树行高调整手柄（树区域底部表头线附近）
            if (local.y - HEADER_HEIGHT).abs() <= 3.0 {
                state.tree_row_resizing = true;
                state.tree_row_resize_start_y = local.y;
                state.tree_row_resize_start_height = state.tree_row_height;
                return Some(canvas::Action::capture());
            }
            // 树区域
            return self.handle_tree_click(state, local);
        }

        // 表格区域
        let table_x = tree_w + SPLITTER_WIDTH;
        let table_local_x = local.x - table_x;
        // 列分隔线
        if let Some(idx) = hit_test::hit_divider(table_local_x, &state.column_widths) {
            state.dragging_divider = Some(idx);
            state.drag_start_x = local.x;
            state.drag_start_widths = state.column_widths.clone();
            return Some(canvas::Action::capture());
        }

        self.handle_table_click(state, local, table_local_x)
    }

    /// 处理右键按下
    pub(super) fn handle_right_press(
        &self,
        state: &mut CanvasState,
        bounds: Rectangle,
        local: Point,
    ) -> Option<canvas::Action<Message>> {
        let tree_w = self.tree_width(bounds, state);
        if local.x < tree_w {
            return None;
        }
        let table_x = tree_w + SPLITTER_WIDTH;
        let table_local_x = local.x - table_x;
        let col = hit_test::hit_test_cell(table_local_x, &state.column_widths)?;
        let rows = detail::collect_rows(
            self.state
                .selected_item
                .as_ref()
                .unwrap_or(&SelectedItem::TimeSig),
            &self.data,
            self.t,
        );
        let (_, page_rows) = self.page_slice(&rows);
        let row_idx = hit_test::hit_test_row(local.y, state.scroll_y)?;
        let row = page_rows.get(row_idx)?;
        let tick = row.tick;
        if col == 0 {
            // 行头右键：打开上下文菜单
            state.context_menu = Some((tick, local));
            return Some(canvas::Action::capture());
        }
        // 单元格右键：发起编辑
        if let Some(request) = row.cell_edits.get(col).and_then(|e| e.clone()) {
            let cell_text = row.cells.get(col).cloned().unwrap_or_default();
            if let Some(popup) = PopupState::from_request(request, &cell_text) {
                state.popup = Some(popup);
                return Some(canvas::Action::capture());
            }
        }
        None
    }

    /// 处理树区域点击
    pub(super) fn handle_tree_click(
        &self,
        state: &mut CanvasState,
        local: Point,
    ) -> Option<canvas::Action<Message>> {
        let idx = hit_test::hit_test_tree(local.y, state.scroll_y, state.tree_row_height)?;
        let all = self.visible_tree_items();
        let item = all.get(idx)?.clone();
        match item {
            TreeItem::Root { key, .. } => {
                Some(canvas::Action::publish(Event::event_list_tree_toggled(key)))
            }
            TreeItem::Track { id, .. } => Some(canvas::Action::publish(
                Event::event_list_tree_toggled(ArchiveKey::Track(id)),
            )),
            TreeItem::Leaf { item, .. } => {
                state.scroll_y = 0.0;
                Some(canvas::Action::publish(Event::event_list_item_selected(
                    item,
                )))
            }
        }
    }

    /// 处理表格区域点击（行头 / 单元格）
    pub(super) fn handle_table_click(
        &self,
        state: &mut CanvasState,
        local: Point,
        table_local_x: f32,
    ) -> Option<canvas::Action<Message>> {
        let rows = detail::collect_rows(
            self.state
                .selected_item
                .as_ref()
                .unwrap_or(&SelectedItem::TimeSig),
            &self.data,
            self.t,
        );
        // 空表格：点击加号插入第一个事件
        if rows.is_empty() {
            return Some(canvas::Action::publish(Event::event_list_edit(
                EditRequest::InsertFirst,
            )));
        }
        let col = hit_test::hit_test_cell(table_local_x, &state.column_widths)?;
        let row_idx = hit_test::hit_test_row(local.y, state.scroll_y)?;
        let (page, page_rows) = self.page_slice(&rows);
        let row = page_rows.get(row_idx)?;
        let tick = row.tick;
        if col == 0 {
            // 行头：单选
            return Some(canvas::Action::publish(Event::event_list_row_clicked(tick)));
        }
        // 单元格：左键打开编辑弹窗（与右键行为一致）
        if let Some(request) = row.cell_edits.get(col).and_then(|e| e.clone()) {
            let cell_text = row.cells.get(col).cloned().unwrap_or_default();
            if let Some(popup) = PopupState::from_request(request, &cell_text) {
                state.popup = Some(popup);
                return Some(canvas::Action::capture());
            }
        }
        // 跳转
        if let Some(jump) = row.cell_jumps.get(col).and_then(|j| j.clone()) {
            let _ = page;
            return Some(canvas::Action::publish(Event::event_list_jump(jump)));
        }
        None
    }

    /// 处理弹窗点击
    pub(super) fn handle_popup_click(
        &self,
        state: &mut CanvasState,
        bounds: Rectangle,
        local: Point,
    ) -> Option<canvas::Action<Message>> {
        match popup::popup_hit_test(local, bounds) {
            PopupHit::Ok => {
                if let Some(popup) = state.popup.take() {
                    let (request, value) = popup.confirm_value();
                    return Some(canvas::Action::publish(Event::event_list_popup_confirm(
                        request, value,
                    )));
                }
                None
            }
            PopupHit::Cancel => {
                state.popup = None;
                Some(canvas::Action::publish(Event::event_list_popup_cancel()))
            }
            PopupHit::ChoicePrev => {
                if let Some(popup) = state.popup.take() {
                    match popup.clone().prev_choice() {
                        Some(next) => state.popup = Some(next),
                        None => state.popup = Some(popup),
                    }
                }
                Some(canvas::Action::request_redraw())
            }
            PopupHit::ChoiceNext => {
                if let Some(popup) = state.popup.take() {
                    match popup.clone().next_choice() {
                        Some(next) => state.popup = Some(next),
                        None => state.popup = Some(popup),
                    }
                }
                Some(canvas::Action::request_redraw())
            }
            PopupHit::None => Some(canvas::Action::capture()),
        }
    }

    /// 处理上下文菜单点击
    pub(super) fn handle_context_menu_click(
        &self,
        state: &mut CanvasState,
        bounds: Rectangle,
        local: Point,
    ) -> Option<canvas::Action<Message>> {
        let (tick, pos) = *state.context_menu.as_ref()?;
        let menu_items = &["Insert Above", "Insert Below", "Delete"];
        let item_h = 22.0;
        for (i, label) in menu_items.iter().enumerate() {
            let rect = Rectangle::new(
                Point::new(pos.x, pos.y + i as f32 * item_h),
                Size::new(140.0, item_h),
            );
            if rect.contains(local) {
                state.context_menu = None;
                let msg = match *label {
                    "Insert Above" => Event::event_list_edit(EditRequest::InsertAbove { tick }),
                    "Insert Below" => Event::event_list_edit(EditRequest::InsertBelow { tick }),
                    _ => Event::event_list_edit(EditRequest::DeleteSelected),
                };
                return Some(canvas::Action::publish(msg));
            }
        }
        // 点击菜单外关闭
        if local.y < pos.y || local.y > pos.y + 3.0 * item_h {
            state.context_menu = None;
        }
        let _ = bounds;
        Some(canvas::Action::capture())
    }

    /// 处理键盘事件
    pub(super) fn handle_keyboard(
        &self,
        state: &mut CanvasState,
        key_event: &iced_core::keyboard::Event,
    ) -> Option<canvas::Action<Message>> {
        // 弹窗打开时：输入转发给弹窗
        if let Some(popup) = state.popup.clone() {
            let action = popup.handle_key(key_event);
            match action {
                crate::sidebar::event_browser::edit::PopupAction::Stay(next) => {
                    state.popup = Some(next);
                    Some(canvas::Action::request_redraw())
                }
                crate::sidebar::event_browser::edit::PopupAction::Confirm((request, value)) => {
                    state.popup = None;
                    Some(canvas::Action::publish(Event::event_list_popup_confirm(
                        request, value,
                    )))
                }
                crate::sidebar::event_browser::edit::PopupAction::Cancel => {
                    state.popup = None;
                    Some(canvas::Action::publish(Event::event_list_popup_cancel()))
                }
            }
        } else {
            match key_event {
                iced_core::keyboard::Event::KeyPressed {
                    key: Key::Named(Named::Delete | Named::Backspace),
                    ..
                } => {
                    if self.state.selected_ticks.is_empty() {
                        None
                    } else {
                        Some(canvas::Action::publish(Event::event_list_edit(
                            EditRequest::DeleteSelected,
                        )))
                    }
                }
                _ => None,
            }
        }
    }
}
