//! 编辑器操作 - 协作功能

use crate::root::Root;

pub mod notes;
pub mod state;

impl Root {
    /// 协作音轨对齐：确保本地 `document` 与侧边栏都覆盖到 `track_idx`
    /// （含中间缺失索引），使来自对端的音符/音轨操作落到正确的音轨，
    /// 避免两方音轨数量不一致时音符落到缺失/错误音轨（音轨错位）或静默丢弃。
    ///
    /// 仅补齐、幂等：已存在则跳过；`document` 为空时静默返回
    /// （协作通常在工程已初始化后进行）。
    pub(crate) fn ensure_collab_track(&mut self, track_idx: usize) {
        // 先扩 document（音符权威源），保持音轨索引与侧边栏一致。
        self.editor.editor_state.data.ensure_track(track_idx);
        while self.sidebar.tracks.len() <= track_idx {
            let id = self.sidebar.tracks.len();
            self.sidebar.tracks.push(crate::sidebar::Track {
                id,
                name: format!("Track {}", id),
                port: 0,
                channel: 0,
                display_label: format!("A{:02}", (id + 1).min(16)),
                is_conductor: false,
                can_delete: true,
                is_muted: false,
                is_soloed: false,
                color: None,
            });
        }
        self.sync_track_visual_order();
    }
}
