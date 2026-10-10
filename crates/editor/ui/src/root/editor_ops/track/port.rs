//! 音轨端口编辑的应用（sidebar → document / 输出布局）
//!
//! sidebar 已即时更新 UI（标签/重排），Root 在 `sidebar.update` 之后消费
//! `pending_track_port_change`，把端口写回 `document.track_ports`——导出/播放
//! 的端口映射与 `max_port` 均从文档推导，只改 UI 不改文档会让听感与显示分叉。
//!
//! 三件配套事（缺一即验收失真）：
//! 1. 刷新引擎文档快照：端口是引擎读取音符/控制事件的唯一事实源，而
//!    `update_playback_notes` 的 `notes_unchanged` 快路径不会触发 `set_document`；
//! 2. `max_port` 变化时通知 Runner 重建实时输出布局（否则新端口事件被丢弃）；
//! 3. 写入失败（无文档/索引越界）时回滚 sidebar，避免 UI 与文档静默分叉。

use std::sync::Arc;

use crate::root::Root;
use crate::sidebar::Sidebar;

impl Root {
    /// 消费 sidebar 待应用的端口编辑：写回文档、置脏、同步视觉映射与播放快照。
    pub(crate) fn forward_pending_track_port_change(&mut self) {
        let Some((track_id, old_port, port)) = self.sidebar.take_pending_track_port_change() else {
            return;
        };

        // 端口编辑前的文档 max_port（用于判断是否需要重建输出布局）。
        let old_max_port = self
            .editor
            .editor_state
            .data
            .document
            .as_ref()
            .map(lumino_midi_loader::MidiDocument::max_port);

        // 写回文档；失败（无文档 / 索引越界）时回滚 sidebar。
        let (new_max_port, applied) = match self.editor.editor_state.data.document.as_mut() {
            Some(doc) => match doc.track_ports.get_mut(track_id) {
                Some(slot) => {
                    *slot = port;
                    (doc.max_port(), true)
                }
                None => {
                    tracing::warn!(
                        "forward_pending_track_port_change: 音轨 {track_id} 超出文档范围（共 {} 轨）",
                        doc.track_ports.len()
                    );
                    (0, false)
                }
            },
            None => {
                tracing::warn!("forward_pending_track_port_change: 无文档，端口编辑未应用");
                (0, false)
            }
        };
        if !applied {
            self.rollback_track_port_change(track_id, old_port);
            return;
        }

        self.editor.editor_state.data.modified = true;
        // 端口编辑改变 sidebar 排序（(port, channel, id)），视觉顺序映射必须同步，
        // 否则走带编辑交互会落到错误音轨。
        self.sync_track_visual_order();
        // 刷新引擎文档快照：`update_playback_notes` 的 notes_unchanged 快路径不会
        // 触发 set_document，不刷新则音符队列与其他轨仍按旧端口映射。
        if let Some(manager) = self.playback.manager.as_mut()
            && let Some(doc) = self.editor.editor_state.data.document.as_ref()
        {
            manager.set_document(
                Arc::new(doc.clone()),
                self.editor.editor_state.data.current_track as u16,
            );
        }
        // 立即重建播放事件映射（当前轨端口 → 全局通道），无需等待下一次编辑触发。
        self.update_playback_notes();

        // max_port 变化 → 实时输出布局需重建（Runner 侧 apply_midi_port_layout）。
        // 仅范围变化才通知：同布局复位会打断正在播放的声部，不应无谓触发。
        if old_max_port != Some(new_max_port) {
            tracing::info!(
                "Root: 端口布局变化 {} -> {new_max_port}，请求重建实时输出",
                old_max_port.unwrap_or(0)
            );
            lumino_message::events::emit(lumino_message::events::Event::Window(
                lumino_message::events::window::Event::midi_port_layout_changed(new_max_port),
            ));
        }

        tracing::info!("音轨 {track_id} 端口已更新为 {port}（显示 {}）", port + 1);
    }

    /// 端口改动无法写入文档时回滚 sidebar（标签/排序），避免 UI 与文档静默分叉。
    fn rollback_track_port_change(&mut self, track_id: usize, old_port: u8) {
        if let Some(track) = self.sidebar.tracks.iter_mut().find(|t| t.id == track_id) {
            track.port = old_port;
            track.display_label = Sidebar::track_label(old_port, track.channel);
        }
        self.sidebar
            .tracks
            .sort_by_key(|t| (t.port, t.channel, t.id));
        self.sync_track_visual_order();
        self.statusbar
            .set_status_message(Some("端口修改未应用：文档不可用".to_string()));
    }
}
