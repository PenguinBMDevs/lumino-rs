//! 洋葱皮构建动作决策 — GPU 全轨模式与 CPU 单轨模式两条分支。
//!
//! GPU 模式（默认）：统一全量渲染语义（全轨常驻 + ViewState 切轨），
//! 增量只作用于被编辑的洋葱皮音轨。
//!
//! CPU 模式（Q3 B）：GPU buffer 只驻留当前轨，因此**只可能**产出 `Full` / `None`：
//! 非当前轨的编辑、静音、调色板变化都不需要（也无法）重传 GPU 数据。

use super::OnionSkinState;
use super::fingerprint::OnionSkinFingerprint;

/// 洋葱皮构建动作（事件级增量，2026-08-05）
///
/// 由 [`OnionSkinState::decide_action`] 决策：
/// - `None`：无需任何操作（数据未变，或变化全部豁免——只改当前/静音音轨）
/// - `Delta(tracks)`：仅这些音轨需要事件级增量替换（段级替换，只传被编辑音轨）
/// - `Full`：全量重建（首次上传 / 调色板 / 未知脏音轨 / force_full 兜底）
/// - `ViewState`：布局变化（切轨/静音）但 GPU 数据未变——只发 `SetViewState`
///   uniform（统一全量渲染 2026-08-06：全量 buffer 常驻所有轨，切轨/静音零重传）
#[derive(Debug)]
pub(crate) enum OnionSkinAction {
    None,
    Delta(Vec<usize>),
    Full,
    ViewState,
}

impl OnionSkinState {
    /// 决策本次洋葱皮构建动作（三态 + ViewState，事件级增量）
    ///
    /// GPU 模式：
    /// - 未初始化 / force_full → `Full`（首次上传 / 未知变化兜底）
    /// - 布局变化（切轨/静音）→ `ViewState`（全量 buffer 常驻所有轨，只发 uniform
    ///   零重传；调色板变化 → `Full`，实例颜色固化需重传）
    /// - 音符数据变化：主轨结构性变化 → 当前轨加入 `Delta`（单轨段重建）；
    ///   脏洋葱皮音轨 → `Delta(音轨)`；脏音轨全豁免 → `None`；
    ///   未知来源 → `Full`（保守正确性）
    /// - 无变化 → `None`
    ///
    /// CPU 模式（`cpu_current_only`）：见 [`Self::decide_action_cpu`]。
    pub(crate) fn decide_action(&self, fp: &OnionSkinFingerprint) -> OnionSkinAction {
        if !self.initialized {
            return OnionSkinAction::Full;
        }
        // 未知变化兜底（undo/redo/散改/加载）：全量会话分块重建段表
        if self.force_full {
            return OnionSkinAction::Full;
        }
        if self.cpu_current_only {
            return self.decide_action_cpu(fp);
        }

        // 布局变化：切轨/静音只影响显示语义（ViewState uniform），数据零重传；
        // 调色板变化需全量重传（实例颜色固化在数据里）
        if fp.mute_fp != self.last_mute_fingerprint || fp.current_track != self.last_current_track {
            return OnionSkinAction::ViewState;
        }
        if fp.palette_idx != self.last_palette_idx {
            return OnionSkinAction::Full;
        }

        // 增量目标：主轨结构性变化（无段内事件可对账）+ 脏洋葱皮音轨
        let mut targets: Vec<usize> = Vec::new();
        if fp.main_track_struct_dirty && fp.current_track < fp.track_count {
            // 守卫：当前轨必须存在于文档中（轨数不足时无段可重建；
            // 轨布局由 TrackLayout 同步后下一帧再走此路径）
            targets.push(fp.current_track);
        }
        if fp.track_gen != self.last_track_notes_gen {
            match &fp.onion_dirty_tracks {
                Some(dirty) if !dirty.is_empty() => {
                    // 统一全量渲染：GPU buffer 常驻所有轨，当前音轨也在其中。
                    // 当前音轨由主轨事件级增量同步，不再走整轨 TrackDelta
                    // （结构性变化除外，已在上方按 `main_track_struct_dirty` 加入）。
                    targets.extend(
                        dirty
                            .iter()
                            .filter(|t| **t != fp.current_track && !fp.muted_tracks.contains(t))
                            .copied(),
                    );
                }
                // None（未知来源）或空集合 → 保守全量重建
                _ => return OnionSkinAction::Full,
            }
        }
        if targets.is_empty() {
            return OnionSkinAction::None;
        }
        OnionSkinAction::Delta(targets)
    }

    /// CPU 单轨模式决策：GPU 数据 = 当前轨一段，只有「段内容/段身份」变化才重传。
    ///
    /// - 切当前轨 / 轨数变化 / 主轨结构性脏 → `Full`（单轨会话重建段表）
    /// - 当前轨普通编辑 → `None`（段内 `NoteEvent` 增量由主轨事件通道负责）
    /// - 洋葱皮轨编辑 / 静音变化 → `None`（GPU 无该轨数据，无需重传）
    /// - 调色板变化 → `None`（当前轨颜色由 shader 主轨蓝强制覆盖，实例色无效）
    /// - 未知来源（`onion_dirty_tracks == None`）→ `Full`（保守正确性）
    ///
    /// 永不返回 `Delta` / `ViewState`：CPU 模式没有多段，也没有「只改显示语义」的
    /// 场景（当前轨身份变化本身就是段身份变化）。
    fn decide_action_cpu(&self, fp: &OnionSkinFingerprint) -> OnionSkinAction {
        if fp.current_track != self.last_current_track
            || fp.track_count != self.last_track_count
            || fp.main_track_struct_dirty
        {
            return OnionSkinAction::Full;
        }
        // 音符代次变化但来源未知 → 保守全量（与 GPU 模式同口径）
        if fp.track_gen != self.last_track_notes_gen && fp.onion_dirty_tracks.is_none() {
            return OnionSkinAction::Full;
        }
        OnionSkinAction::None
    }
}
