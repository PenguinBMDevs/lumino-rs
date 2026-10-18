//! 音轨静音/独奏的应用（sidebar → document）
//!
//! `.lmpj` 保存链路（`LuminoProject::from_midi_document`，见
//! `crates/core/project/src/project/conversion.rs`）从 `document.tracks` 读取每轨
//! `visibility` / `solo` 落盘；而 UI 的静音/独奏开关只改
//! `sidebar.tracks[].is_muted` / `is_soloed`。两侧此前**没有任何同步**：
//! 用户在 UI 里静音/独奏某轨 → 保存 `.lmpj` → 重开，拿到的是**加载时**的旧状态。
//! 这与「CC/PB 听得到、存不下来」（审计 §一）是同一类缺陷：编辑态与保存源分叉。
//!
//! 两条不可退让的契约：
//!
//! 1. **逐轨写，绝不整批写**。加载工程时 sidebar 的音轨一律以
//!    `is_muted: false` / `is_soloed: false` 构造（`update_tracks_from_midi` 只携带
//!    名称/通道/端口），UI 并未回填已保存的静音/独奏。若在这里做全量同步，
//!    「加载一个含静音轨的工程 → 随手切一下别的轨」就会把其它轨已保存的
//!    静音/独奏全部抹平。
//! 2. **`TrackVisibility::Hidden` 不得被静音开关降级**。Hidden 只存在于 `.lmpj`
//!    元数据，UI 未建模（`Track` 只有静音/独奏两个布尔位）；一次静音切换若按
//!    `is_muted` 二值重写成 `Visible`/`Muted`，隐藏轨就会被动变可见。
//!
//! 已知边界（未在本次范围内）：UI 不回填加载工程的静音/独奏，界面上看不到
//! 「已保存的静音」——那是显示层缺口，与本模块的写回方向不同，且在本次修复前
//! 就存在。

use lumino_midi_loader::TrackVisibility;

use crate::root::Root;

impl Root {
    /// 把 **单条**音轨的静音/独奏状态写回 `document.tracks`（`.lmpj` 保存源）。
    ///
    /// `track_id` 是文档音轨索引（`sidebar::Track.id`），**不是**列表行号——
    /// 拖拽排序后两者不同（视觉行 → 文档索引由 `track_visual_order` 映射）。
    pub(crate) fn apply_track_visibility_to_document(&mut self, track_id: usize) {
        // 先取出两个布尔位，结束对 sidebar 的不可变借用
        let Some((is_muted, is_soloed)) = self
            .sidebar
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .map(|track| (track.is_muted, track.is_soloed))
        else {
            return;
        };

        let Ok(track_id) = u16::try_from(track_id) else {
            tracing::warn!("apply_track_visibility_to_document: 音轨索引 {track_id} 超出 u16");
            return;
        };

        let Some(doc) = self.editor.editor_state.data.document.as_mut() else {
            // 无文档 = 无工程可保存：UI/播放仍按 sidebar 生效，无需回滚
            tracing::warn!("apply_track_visibility_to_document: 无文档，静音/独奏未写入保存源");
            return;
        };

        let existing = doc.tracks.get(track_id).map(|view| view.visibility);
        let visibility = match (existing, is_muted) {
            // Hidden 是 UI 未建模的持久化状态：静音开关不得把它降级
            (Some(TrackVisibility::Hidden), _) => TrackVisibility::Hidden,
            (_, true) => TrackVisibility::Muted,
            (_, false) => TrackVisibility::Visible,
        };
        doc.tracks.set_visibility(track_id, visibility);
        doc.tracks.set_solo(track_id, is_soloed);

        // 写入保存源 = 工程内容变化（否则「切了静音但没动音符」不置脏，保存被跳过）
        self.editor.editor_state.data.modified = true;
    }
}
