//! 事件列表处理 — EventList 相关事件
//!
//! 本模块只修改 `Sidebar` 自身状态，所有需要写入 `EditorData` 的操作
//! 都通过 `pending_event_list_action` / `pending_event_list_edit` 缓存，
//! 供 `Root::handle_sidebar_event` 在 `sidebar.update()` 之后消费。

use crate::sidebar::core::Sidebar;
use crate::sidebar::event_browser::{
    ArchiveKey, EditRequest, EventListAction, EventListMenuItem, JumpRequest, SelectedItem,
    TextEventKind,
};

impl Sidebar {
    /// 处理事件列表滚动偏移与视口高度更新
    pub(super) fn handle_event_list_scrolled(&mut self, offset: f32, viewport_height: f32) {
        self.event_list_scroll_y = offset.max(0.0);
        self.event_list_viewport_height = viewport_height.max(0.0);
    }

    /// 处理事件列表行点击：单选当前行并设置跳转锚点。
    pub(super) fn handle_event_list_row_clicked(&mut self, tick: u32) {
        let state = &mut self.event_browser_state;
        state.selected_ticks.clear();
        state.selected_ticks.insert(tick);
        state.last_clicked_tick = Some(tick);
    }

    /// 处理事件列表右键行头打开上下文菜单。
    pub(super) fn handle_event_list_context_menu_opened(&mut self, tick: u32) {
        self.event_list_context_menu_tick = Some(tick);
    }

    /// 处理事件列表关闭上下文菜单。
    pub(super) fn handle_event_list_context_menu_closed(&mut self) {
        self.event_list_context_menu_tick = None;
    }

    /// 处理事件列表上下文菜单项点击：转换为待执行操作。
    pub(super) fn handle_event_list_context_menu_item_clicked(&mut self, item: EventListMenuItem) {
        let Some(tick) = self.event_list_context_menu_tick else {
            return;
        };
        self.pending_event_list_action = Some(match item {
            EventListMenuItem::InsertAbove => EventListAction::InsertAbove(tick),
            EventListMenuItem::InsertBelow => EventListAction::InsertBelow(tick),
            EventListMenuItem::Delete => EventListAction::DeleteSelected,
        });
        self.event_list_context_menu_tick = None;
    }

    /// 处理事件列表跳转请求：实际跳转逻辑由 Root 完成。
    pub(super) fn handle_event_list_jump(&mut self, _req: JumpRequest) {
        // Root 负责切换音轨、设置光标位置等。
    }

    /// 处理事件列表编辑/操作请求。
    ///
    /// 直接操作（删除/插入/文本）立即生成 `EventListAction`；
    /// 需要 popup 的编辑由 UI 层打开 popup，此处不缓存任何状态。
    pub(super) fn handle_event_list_edit(&mut self, req: EditRequest) {
        match req {
            EditRequest::DeleteSelected => {
                self.pending_event_list_action = Some(EventListAction::DeleteSelected);
            }
            EditRequest::InsertAbove { tick } => {
                self.pending_event_list_action = Some(EventListAction::InsertAbove(tick));
            }
            EditRequest::InsertBelow { tick } => {
                self.pending_event_list_action = Some(EventListAction::InsertBelow(tick));
            }
            EditRequest::InsertFirst => {
                self.pending_event_list_action = Some(EventListAction::InsertFirst);
            }
            _ => {
                // 需要 popup 或 Root 级数据访问的编辑，保留原始请求。
                self.pending_event_list_edit = Some((req, String::new()));
            }
        }
    }

    /// 处理事件列表 popup 编辑器确认：解析值并生成操作。
    ///
    /// 不依赖 `EditorData` 的解析（如音符属性、文本）直接生成 `EventListAction`；
    /// 需要读取当前数据的解析（如拍号分母、自动化目标）保留原始请求，
    /// 由 `Root` 在获得 `EditorData` 访问权后完成。
    pub(super) fn handle_event_list_popup_confirm(&mut self, req: EditRequest, value: String) {
        match req {
            EditRequest::NoteStartTick { note } => {
                if let Ok(new_tick) = value.parse::<u32>() {
                    self.pending_event_list_action =
                        Some(EventListAction::SetNoteStart { note, new_tick });
                }
            }
            EditRequest::NoteEndTick { note } => {
                if let Ok(new_end_tick) = value.parse::<u32>() {
                    self.pending_event_list_action =
                        Some(EventListAction::SetNoteEnd { note, new_end_tick });
                }
            }
            EditRequest::NoteGate { note } => {
                if let Ok(gate) = value.parse::<f32>() {
                    self.pending_event_list_action =
                        Some(EventListAction::SetNoteGate { note, gate });
                }
            }
            EditRequest::NoteKey { note } => {
                if let Ok(new_key) = value.parse::<u8>() {
                    self.pending_event_list_action =
                        Some(EventListAction::SetNoteKey { note, new_key });
                }
            }
            EditRequest::NoteVelocity { note } => {
                if let Ok(new_velocity) = value.parse::<u8>() {
                    self.pending_event_list_action =
                        Some(EventListAction::SetNoteVelocity { note, new_velocity });
                }
            }
            EditRequest::TextEventText { kind, tick } => {
                self.pending_event_list_action = Some(match kind {
                    TextEventKind::Marker => EventListAction::SetMarker { tick, text: value },
                    TextEventKind::ConductorLyrics => EventListAction::SetLyrics {
                        track: 0,
                        tick,
                        text: value,
                    },
                    TextEventKind::ConductorChord => EventListAction::SetChord {
                        track: 0,
                        tick,
                        text: value,
                    },
                    TextEventKind::Lyrics { track } => EventListAction::SetLyrics {
                        track,
                        tick,
                        text: value,
                    },
                    TextEventKind::Chord { track } => EventListAction::SetChord {
                        track,
                        tick,
                        text: value,
                    },
                });
            }
            _ => {
                // 需要 Root 级数据访问的 popup，保留原始请求待解析。
                self.pending_event_list_edit = Some((req, value));
            }
        }
    }

    /// 处理事件列表 popup 编辑器取消。
    pub(super) fn handle_event_list_popup_cancel(&mut self) {
        // UI 层关闭 popup，无需修改 Sidebar 状态。
    }

    /// 处理事件浏览器树节点展开/折叠切换。
    pub(super) fn handle_event_list_tree_toggled(&mut self, key: ArchiveKey) {
        let state = &mut self.event_browser_state;
        if !state.expanded_keys.remove(&key) {
            state.expanded_keys.insert(key);
        }
    }

    /// 处理事件浏览器选中树叶子项。
    pub(super) fn handle_event_list_item_selected(&mut self, item: SelectedItem) {
        let state = &mut self.event_browser_state;
        state.selected_item = Some(item);
        state.event_page = 0;
        state.selected_ticks.clear();
        state.last_clicked_tick = None;
    }

    /// 处理事件浏览器翻页。
    pub(super) fn handle_event_list_page_changed(&mut self, page: usize) {
        self.event_browser_state.event_page = page;
    }
}
