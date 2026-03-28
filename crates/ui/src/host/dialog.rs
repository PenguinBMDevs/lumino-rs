//! Host 对话框和协作子模块 - 处理对话框状态和远程协作

use crate::host::{Host, types::DialogResult};
use crate::state::root_state::CollaborationViewState;
use crate::{message, window};

impl Host {
    /// 设置自定义精度对话框是否打开（用于独立对话框窗口）
    pub fn set_custom_precision_dialog_open(&mut self, open: bool) {
        self.root.set_custom_precision_dialog_open(open);
        self.clear_cache();
        self.window.request_redraw();
    }

    /// 获取并清空对话框结果
    pub fn take_dialog_result(&mut self) -> Option<DialogResult> {
        self.root.take_dialog_result()
    }

    /// 设置自定义精度值（用于独立对话框窗口）
    pub fn set_custom_precision(&mut self, ticks: f32) {
        self.root.set_custom_precision(ticks);
        self.clear_cache();
        self.window.request_redraw();
    }

    /// 设置协作对话框是否打开（用于独立对话框窗口）
    pub fn set_collaboration_dialog_open(&mut self, open: bool) {
        self.root.set_collaboration_dialog_open(open);
        self.clear_cache();
        self.window.request_redraw();
    }

    /// 设置协作视图状态（用于独立对话框窗口）
    pub fn set_collaboration_view_state(
        &mut self,
        state: CollaborationViewState,
        invite_code: Option<String>,
        room_name: Option<String>,
    ) {
        self.root
            .set_collaboration_view_state(state, invite_code, room_name);
        self.clear_cache();
        self.window.request_redraw();
    }

    /// 更新远端鼠标位置
    pub fn update_remote_cursor(
        &mut self,
        user_id: String,
        x: f32,
        y: f32,
        color: String,
        username: String,
    ) {
        self.root
            .update(message::Message::CollaborationRemoteMouseMoved {
                user_id,
                x,
                y,
                color,
                username,
            });
        self.clear_cache();
        self.window.request_redraw();
    }

    /// 移除远端鼠标
    pub fn remove_remote_cursor(&mut self, user_id: String) {
        self.root
            .update(message::Message::CollaborationRemoteUserLeft { user_id });
        self.clear_cache();
        self.window.request_redraw();
    }

    /// 更新远端音符
    pub fn update_remote_note(&mut self, user_id: String, operation: String) {
        self.root
            .update(message::Message::CollaborationRemoteNoteUpdate { user_id, operation });
        self.clear_cache();
        self.window.request_redraw();
    }
    /// 应用远程笔记操作到本地编辑器
    pub fn apply_remote_note_operation(
        &mut self,
        operation: &lumino_collaboration::types::NoteBatchOperation,
    ) {
        use lumino_collaboration::types::NoteAction;

        match operation.action {
            NoteAction::Add => {
                for note in &operation.notes {
                    // 转换协作音符为编辑器音符
                    let editor_note =
                        crate::editor::note::Note::new(note.tick, note.key, note.length);

                    // 添加到对应的音轨
                    let track_idx = note.track_index;
                    if track_idx == self.root.editor.current_track {
                        // 如果是当前音轨，直接添加到编辑器
                        self.root.editor.notes.push(editor_note.clone());
                        self.root.editor.grid_cache.clear();
                    }

                    // 更新 track_notes
                    let track_notes = self
                        .root
                        .editor
                        .track_notes
                        .entry(track_idx)
                        .or_insert_with(Vec::new);
                    track_notes.push(editor_note);
                }
                tracing::info!("协作: 已添加 {} 个远程音符", operation.notes.len());
            }
            NoteAction::Update => {
                // 更新操作：根据 note.id 查找并更新现有音符
                // 由于编辑器音符没有 id，我们暂时基于位置匹配
                for note in &operation.notes {
                    if let Some(track_notes) =
                        self.root.editor.track_notes.get_mut(&note.track_index)
                    {
                        for editor_note in track_notes.iter_mut() {
                            // 基于 tick 和 key 匹配（简化匹配）
                            if (editor_note.tick - note.tick).abs() < 1.0
                                && editor_note.key == note.key
                            {
                                editor_note.length = note.length;
                                editor_note.key = note.key;
                                break;
                            }
                        }
                    }
                }
                self.root.editor.grid_cache.clear();
                tracing::info!("协作: 已更新 {} 个远程音符", operation.notes.len());
            }
            NoteAction::Delete => {
                // 删除操作：根据位置匹配删除音符
                for note in &operation.notes {
                    if let Some(track_notes) =
                        self.root.editor.track_notes.get_mut(&note.track_index)
                    {
                        track_notes
                            .retain(|n| !((n.tick - note.tick).abs() < 1.0 && n.key == note.key));
                    }
                }
                // 同时更新当前显示的音符
                if let Some(source_track) = operation.source_track {
                    if source_track == self.root.editor.current_track {
                        self.root.editor.notes = self
                            .root
                            .editor
                            .track_notes
                            .get(&source_track)
                            .cloned()
                            .unwrap_or_default();
                    }
                }
                self.root.editor.grid_cache.clear();
                tracing::info!("协作: 已删除 {} 个远程音符", operation.notes.len());
            }
            _ => {
                tracing::debug!("协作: 未处理的笔记操作类型: {:?}", operation.action);
            }
        }

        self.window.request_redraw();
    }

    /// 获取当前 PPQ (Pulses Per Quarter note)
    pub fn ppq(&self) -> u16 {
        self.root.editor.state.ppq
    }

    /// 更新进度
    pub fn update_progress(&mut self, progress: Option<(String, f64)>) {
        self.root.update(message::Message::Progress(progress));
    }

    /// 更新主题
    pub fn update_theme(&mut self, theme: String) {
        self.root.update(window::Event::theme(theme));
        self.clear_cache();
        self.window.request_redraw();
    }

    /// 打开协作对话框并设置为连接中状态（用于调试模式自动连接）
    pub fn open_collaboration_dialog_with_state(
        &mut self,
        host: String,
        port: u16,
        username: String,
    ) {
        self.root
            .open_collaboration_dialog_with_state(host, port, username);
        self.clear_cache();
        self.window.request_redraw();
    }

    /// 从另一个 Host 同步协作状态（用于对话框窗口同步主窗口状态）
    pub fn sync_collaboration_state_from(&mut self, other: &Host) {
        self.root.sync_collaboration_state_from(&other.root);
        self.clear_cache();
        self.window.request_redraw();
    }
}
