//! 音轨端口选择处理 — 打开选择器、选择端口、关闭
//!
//! 端口内部值 `0..=15`（产品上限 16 端口），UI 显示 `1..=16`
//! （对齐 Domino 与各类 DAW 的端口编号惯例）。

use crate::sidebar::core::{Sidebar, TrackContextMenuState};

impl Sidebar {
    /// 处理打开端口选择器
    pub(super) fn handle_track_port_picker_opened(&mut self, id: usize) {
        self.port_picking_track = Some(id);
        self.track_context_menu = TrackContextMenuState::default();
        self.renaming_track = None;
        self.color_picking_track = None;
    }

    /// 处理选择音轨端口（内部值，UI 显示值为其 +1）。
    ///
    /// 同步更新标签并按 `(port, channel, id)` 重排（与导入后的排序口径一致），
    /// 同时写入 pending 供 Root 应用到 document（导出/播放语义依赖文档端口）。
    pub(super) fn handle_track_port_selected(&mut self, id: usize, port: u8) {
        self.port_picking_track = None;

        // 防御性收口：端口上限 0..=15（产品 16 端口），越界输入夹紧。
        let port = port.min(Sidebar::PORT_CHOICES - 1);

        let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) else {
            return;
        };
        // 指挥轨不发音符、无端口语义，忽略编辑请求。
        if track.is_conductor {
            return;
        }
        track.port = port;
        track.display_label = Self::track_label(port, track.channel);

        // 与 `update_tracks_from_midi` 同口径：port → channel → id。
        // 编辑端口会改变排序键，必须重排以保持列表与数据一致。
        self.tracks.sort_by_key(|t| (t.port, t.channel, t.id));

        self.pending_track_port_change = Some((id, port));
    }

    /// 处理关闭端口选择器
    pub(super) fn handle_track_port_picker_closed(&mut self, _id: usize) {
        self.port_picking_track = None;
    }
}
