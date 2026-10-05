//! 音轨端口编辑的应用（sidebar → document）
//!
//! sidebar 已即时更新 UI（标签/重排），Root 在 `sidebar.update` 之后消费
//! `pending_track_port_change`，把端口写回 `document.track_ports`——导出/播放
//! 的端口映射与 `max_port` 均从文档推导，只改 UI 不改文档会让听感与显示分叉。

use crate::root::Root;

impl Root {
    /// 消费 sidebar 待应用的端口编辑：写回文档、置脏并同步视觉位置映射。
    pub(crate) fn forward_pending_track_port_change(&mut self) {
        let Some((track_id, port)) = self.sidebar.take_pending_track_port_change() else {
            return;
        };
        let Some(doc) = self.editor.editor_state.data.document.as_mut() else {
            tracing::warn!("forward_pending_track_port_change: 无文档，丢弃端口编辑 id={track_id}");
            return;
        };
        if let Some(slot) = doc.track_ports.get_mut(track_id) {
            *slot = port;
        } else {
            tracing::warn!(
                "forward_pending_track_port_change: 音轨 {track_id} 超出文档范围（共 {} 轨），已丢弃",
                doc.track_ports.len()
            );
            return;
        }
        self.editor.editor_state.data.modified = true;
        // 端口编辑改变 sidebar 排序（(port, channel, id)），视觉顺序映射必须同步，
        // 否则走带编辑交互会落到错误音轨。
        self.sync_track_visual_order();
        // 立即重建播放事件映射（当前轨端口 → 全局通道），无需等待下一次编辑触发。
        self.update_playback_notes();
        tracing::info!("音轨 {track_id} 端口已更新为 {port}（显示 {}）", port + 1);
    }
}
