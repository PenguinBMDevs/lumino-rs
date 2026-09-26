//! 工程走带选中音符**查询**与视图无关选区解析
//!
//! 提供以下操作：
//! - `arrange_select_all_notes`: 全选（全部音轨 × 全部 tick）
//! - `arrangement_selected_notes`: 获取选中音符列表（用于 ghost 预览）
//! - `resolve_selection` / `has_active_selection`: 视图无关的选区解析
//!   （统一入口，见 [`crate::edit_view`]）
//!
//! 2026-08 单一权威源：音符唯一权威是 document，本模块直接读 MidiDocument，
//! 不再维护 track_notes 缓存。
//!
//! **批量写操作**（删除 / 变速）在 `selection/batch.rs`——读路径与写路径分离，
//! 避免读路径的缓存约束被写路径细节淹没。

mod batch;

use lumino_midi_loader::NoteEvent;

use super::super::edit_view::{EditView, SelectionSnapshot};
use super::Editor;

impl Editor {
    /// 工程走带全选：选中**全部音轨 × 全部 tick 区间**。
    ///
    /// 走带选区是跨轨矩形，故全选天然覆盖整张工程——与钢琴卷帘
    /// `select_all_notes`（仅当前轨全部音符）语义对齐：都是「当前视图内的一切」。
    ///
    /// 使用矩形模式（非冻结集）：全选是纯几何操作，无需精确成员集合，
    /// 且矩形模式在撤销后仍是活语义（见 `invalidate_arrange_selection_after_history`）。
    ///
    /// 返回是否建立了全选（无工程 / 无音轨时为 `false`）。
    pub fn arrange_select_all_notes(&mut self) -> bool {
        let data = &mut self.editor_state.data;
        let Some(doc) = data.document.as_ref() else {
            tracing::debug!("Arrangement: 全选 - 无工程文档");
            return false;
        };
        let track_count = doc.track_count();
        if track_count == 0 {
            tracing::debug!("Arrangement: 全选 - 工程无音轨");
            return false;
        }
        // tick 上界取工程实际最大终点；空工程兜底一个最小可见区间，
        // 否则 te <= ts 会让 add_rect_track 静默拒绝、用户看不到任何选区反馈
        let max_tick = doc.tracks_max_end_tick().max(1);
        data.arrange_selection.clear();
        data.arrange_selection.add_rect_track(
            0,
            max_tick,
            0,
            127,
            0,
            (track_count - 1).min(u16::MAX as usize) as u16,
        );
        tracing::info!(
            "Arrangement: 全选 {} 条音轨（tick 0..{max_tick}）",
            track_count
        );
        true
    }

    /// 获取当前工程走带选择范围内的音符列表。
    ///
    /// 返回 `(tick_start, tick_end, track, key)`，用于 ghost 预览。
    /// track 为视觉位置（侧边栏顺序），而非文档音轨索引。
    /// ghost 计算中的 dtr 是视觉空间偏移，与视觉位置相加得到正确的渲染位置。
    ///
    /// 2026-09 性能修复：改走 `selection_cache` 的 `Rc` 共享结果。
    /// 旧实现每帧全量扫描全文档音符（框选后 12.9ms/帧），而唯一消费者
    /// `compute_ghost_notes` 只在「正在拖动已有选区」（`move_drag.is_some()`）
    /// 时才读它——其余帧的扫描产出即扔。
    pub fn arrangement_selected_notes(&self) -> std::rc::Rc<[(f64, f64, usize, u8)]> {
        self.arrangement_selection_hits().flat()
    }

    /// 解析**当前视图**的选中音符，返回视图无关快照（导出 / 闸门 / 菜单可用性共用）。
    ///
    /// 这是选区解析的**唯一入口**：调用方必须显式给出 [`EditView`]，无法绕过视图
    /// 直接读某一套选区——从架构上消除「新命令忘记判视图」的可能。
    ///
    /// # 语义
    ///
    /// - 主选区（`view` 指定的视图）有命中 → 返回它；
    /// - 主选区为空 → **回退另一套**。回退是必需的：菜单项启用条件是
    ///   「任一选区非空」（OR），若此处不回退，就会出现「菜单能点、导不出」，
    ///   或「闸门放行、实际操作了另一个视图的选区」。
    ///
    /// # P0 修复（此前 `Host::get_selected_notes` 内的三处逻辑错误）
    ///
    /// 1. **优先序错误**：旧实现 `if has_selection() { return; }` 让卷帘选区
    ///    无条件优先，走带选区被完全忽略；而菜单按 OR 判定可点 → 用户框了 A、
    ///    导出得到 B。两套选区互不清理（从卷帘切到走带后卷帘选区仍留存），
    ///    该错误极易触发。
    /// 2. **坐标空间错误**：旧走带分支把**文档音轨索引**当**视觉音轨**传进
    ///    `ArrangeSelection::contains`。选区（含冻结集）存的是视觉轨
    ///    （见 `move_notes::frozen_entries_of_moved` 的 `visual_position_of` 转换），
    ///    `track_visual_order` 非恒等时判定全错——与 2025-07 修过的
    ///    `arrangement-y-axis-movement` 是**同一个坑换个入口复现**。
    pub fn resolve_selection(&self, view: EditView) -> SelectionSnapshot {
        let resolved = match view {
            EditView::Arrangement => {
                let arranged = self.collect_arrangement_selected_notes();
                if arranged.is_empty() {
                    self.collect_roll_selected_notes()
                } else {
                    arranged
                }
            }
            EditView::PianoRoll => {
                let roll = self.collect_roll_selected_notes();
                if roll.is_empty() {
                    self.collect_arrangement_selected_notes()
                } else {
                    roll
                }
            }
        };
        SelectionSnapshot::new(view, resolved)
    }

    /// 当前视图选区是否可用（有**实际命中音符**，非「选区结构非空」）
    ///
    /// 工具栏批量操作闸门与菜单项可用性都应以此为准：判据是「有没有可操作的
    /// 对象」，否则会出现「能点却什么都不做」。
    ///
    /// 2026-09 性能修复：判空**零分配**。旧实现走 `resolve_selection`，
    /// 仅为取一个 bool 却把命中音符全量拷贝进 `Vec<NoteEvent>`——
    /// 而 `view_main` 每帧调用它（导出素材菜单可用性），框选后单帧 10.6ms。
    /// 现直接查派生缓存的 `is_empty()`，走带分支与 `resolve_selection`
    /// 同源（同一份缓存），`tests/arrangement_history.rs` 有同源一致性回归。
    pub fn has_active_selection(&self, view: EditView) -> bool {
        match view {
            // 走带优先：命中即真，避免无谓的回退扫描
            EditView::Arrangement => {
                if self.arrangement_selection_hits().is_empty() {
                    !self.collect_roll_selected_notes().is_empty()
                } else {
                    true
                }
            }
            EditView::PianoRoll => {
                if self.collect_roll_selected_notes().is_empty() {
                    !self.arrangement_selection_hits().is_empty()
                } else {
                    true
                }
            }
        }
    }

    /// 走带选区命中的音符（跨轨，**视觉空间**判定）
    ///
    /// 2026-09 性能修复：改走 `selection_cache`，键命中时零扫描。
    fn collect_arrangement_selected_notes(&self) -> Vec<(usize, Vec<NoteEvent>)> {
        self.arrangement_selection_hits().by_track().to_vec()
    }

    /// 卷帘选区命中的音符（单轨，索引位图）
    fn collect_roll_selected_notes(&self) -> Vec<(usize, Vec<NoteEvent>)> {
        let editor_data = &self.editor_state.data;
        if !self.has_selection() {
            return Vec::new();
        }
        let track = editor_data.current_track;
        let notes = editor_data.track_notes(track);
        let selected: Vec<NoteEvent> = self
            .get_selected_indices()
            .into_iter()
            .filter_map(|idx| notes.get(idx).copied())
            .collect();
        if selected.is_empty() {
            Vec::new()
        } else {
            vec![(track, selected)]
        }
    }
}
