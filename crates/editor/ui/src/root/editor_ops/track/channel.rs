//! 音轨通道编辑的应用（sidebar → document）
//!
//! 通道是**内容属性**：音符、控制事件（CC/PC/PB）与自动化 lane 都携带通道号。
//! sidebar 只即时更新 UI（标签/重排），真实数据改写在这里完成，保证
//! 「选通道 → 播放/导出生效」而不是只改显示。

use crate::root::Root;
use crate::sidebar::Sidebar;

impl Root {
    /// 消费 sidebar 待应用的通道编辑：批量改写该轨音符/控制事件/自动化 lane
    /// 的通道，置脏并刷新播放映射。
    pub(crate) fn forward_pending_track_channel_change(&mut self) {
        let Some((track_id, old_channel, new_channel)) =
            self.sidebar.take_pending_track_channel_change()
        else {
            return;
        };
        let new_channel = new_channel.min(Sidebar::CHANNEL_CHOICES - 1);

        // 1) 音符：整轨替换（走 EditorData，保持变更标记/洋葱皮语义）。
        let mapped: Option<Vec<lumino_midi_loader::NoteEvent>> = self
            .editor
            .editor_state
            .data
            .document
            .as_ref()
            .and_then(|doc| doc.notes.get(track_id))
            .map(|notes| {
                notes
                    .iter()
                    .map(|note| {
                        let mut note = *note; // NoteEvent: Copy
                        note.channel = new_channel;
                        note
                    })
                    .collect()
            });
        let Some(mapped) = mapped else {
            self.rollback_track_channel_change(track_id, old_channel);
            return;
        };
        self.editor
            .editor_state
            .data
            .replace_track_notes(track_id, mapped);

        // 2) 控制事件（CC/PC/PB）：同轨全部改道（PackedControlEvent 字段可写）。
        if let Some(doc) = self.editor.editor_state.data.document.as_mut() {
            let mut events: Vec<_> = doc.control_events.iter().copied().collect();
            let mut changed = false;
            for event in events.iter_mut() {
                if event.track == track_id as u16 && event.channel != new_channel {
                    event.channel = new_channel;
                    changed = true;
                }
            }
            if changed {
                doc.control_events = lumino_midi_loader::ChunkedList::from_sorted(events);
            }
        }

        // 3) 自动化 lane：用户编辑的 CC/PB 以 lane 为播放源，通道必须同步，
        //    否则改完通道后 lane 事件仍走旧全局通道（听感与显示分叉）。
        //    lane 为 `Arc<AutomationLane>`（写时复制），经 `Arc::make_mut` 修改。
        for lane in &mut self.editor.editor_state.data.automation_lanes {
            if lane.track == track_id as u16 {
                std::sync::Arc::make_mut(lane).channel = new_channel;
            }
        }

        self.editor.editor_state.data.modified = true;
        // 通道改变排序键（(port, channel, id)），视觉顺序映射必须同步。
        self.sync_track_visual_order();
        // 音符/控制事件/lane 均已变更：重建当前轨队列与混音路由。
        self.update_playback_notes();
        self.update_playback_track_mix();
        tracing::info!(
            "音轨 {track_id} 通道已更新为 {new_channel}（显示 {}）",
            new_channel + 1
        );
    }

    /// 通道改动无法写入文档时回滚 sidebar（标签/排序），避免 UI 与文档静默分叉。
    fn rollback_track_channel_change(&mut self, track_id: usize, old_channel: u8) {
        if let Some(track) = self.sidebar.tracks.iter_mut().find(|t| t.id == track_id) {
            track.channel = old_channel;
            track.display_label = Sidebar::track_label(track.port, old_channel);
        }
        self.sidebar
            .tracks
            .sort_by_key(|t| (t.port, t.channel, t.id));
        self.sync_track_visual_order();
        self.statusbar
            .set_status_message(Some("通道修改未应用：文档不可用".to_string()));
    }
}
