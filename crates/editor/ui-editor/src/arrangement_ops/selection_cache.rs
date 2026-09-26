//! 工程走带选区命中音符的**派生缓存**
//!
//! # 为什么需要（2026-09 性能修复）
//!
//! 走带选区命中的音符是 `(document, arrange_selection, track_visual_order)` 的
//! **纯函数**。此前它在 view 层被**每帧**重算两次：
//!
//! | 调用点 | 代价 | 真实需要 |
//! |--------|------|---------|
//! | `view_arrangement` → `arrangement_selected_notes()` | 全量扫描全文档音符 + `Vec` 分配（12.9ms） | ghost 预览，且仅「正在拖动已有选区」时 |
//! | `view_main` → `active_selection().is_empty()` | 全量扫描 + `NoteEvent` 全量拷贝（10.6ms） | 一个 `bool` |
//!
//! 两处都被 `ArrangeSelection::is_empty()` 门控——**空选区是 O(1) 早退**，
//! 所以「框选后变慢」的表象掩盖了真因：不是框选本身贵，而是**选区一旦非空，
//! 每帧都要重扫整份文档**。而输入在选区不变时根本不变。
//!
//! # 修法
//!
//! 用「版本号三元组」做失效键，命中即 O(1)：
//! - `EditorData::track_notes_gen` —— 音符数据变了（沿用代码库既有三处范式）
//! - `ArrangeSelection::revision()` —— 选区变了（含仅几何平移，音符数据不变）
//! - `track_visual_order` 指纹 —— 侧边栏排序变了（视觉位置映射随之改变）
//!
//! 结果：每帧 O(1) 命中，O(全文档音符) 只在**三者之一真正变化**时付一次。
//!
//! # 为什么轨道序用指纹而不是计数器
//!
//! `track_visual_order` 是 `pub` 字段，在 `reset.rs` / `Root::sync_track_visual_order`
//! / 测试中多处直接赋值，没有单一写入入口可以挂计数器。音轨数量级是几十到几百，
//! 指纹代价可忽略，换来的是**不可能漏 bump**——漏 bump 会静默读到脏数据，
//! 那比多算一次糟糕得多。

use std::cell::{Ref, RefCell};
use std::rc::Rc;

use lumino_midi_loader::NoteEvent;

use super::Editor;

/// 走带选区命中音符（一次扫描产出两种消费形态）
///
/// 两种形态来自同一次扫描，避免两处调用点各扫一遍全文档：
/// - `by_track`：按**文档音轨**分组，供 `SelectionSnapshot`（导出 / 剪贴板 / 批量操作）
/// - `flat`：`Rc` 共享的 `(tick_start, tick_end, 视觉轨, key)` 扁平表，供 ghost 预览
#[derive(Debug, Default)]
pub(crate) struct ArrangeSelectionHits {
    /// 缓存失效键（`track_notes_gen`, `arrange_selection.revision`, 轨道序指纹）
    key: (u64, u64, u64),
    /// 按文档音轨分组的命中音符（仅含非空轨）
    by_track: Vec<(usize, Vec<NoteEvent>)>,
    /// 扁平命中表（ghost 预览用）；`Rc` 让 view 层每帧克隆只是引用计数自增
    flat: Rc<[(f64, f64, usize, u8)]>,
}

impl ArrangeSelectionHits {
    /// 按文档音轨分组的命中音符（仅含非空轨），供快照构造。
    pub(crate) fn by_track(&self) -> &[(usize, Vec<NoteEvent>)] {
        &self.by_track
    }

    /// 是否零命中（零分配判空——`view_main` 每帧只需要这个）。
    pub(crate) fn is_empty(&self) -> bool {
        self.by_track.is_empty()
    }

    /// 扁平命中表（ghost 预览用），克隆 `Rc` 为 O(1)。
    pub(crate) fn flat(&self) -> Rc<[(f64, f64, usize, u8)]> {
        Rc::clone(&self.flat)
    }
}

/// `Editor` 上的走带选区命中缓存槽
pub(crate) type ArrangeSelectionCache = RefCell<Option<ArrangeSelectionHits>>;

/// 构造空缓存槽
pub(crate) fn new_arrange_selection_cache() -> ArrangeSelectionCache {
    RefCell::new(None)
}

/// 计算 `track_visual_order` 的变更指纹（O(音轨数)，远小于音符量级）。
#[inline]
fn visual_order_fingerprint(order: &[usize]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h: u64 = FNV_OFFSET;
    for &id in order {
        h ^= id as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    // 长度参与指纹：前缀相同、长度不同视为不同映射
    h ^ (order.len() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
}

impl Editor {
    /// 走带选区命中音符（缓存版，键命中时零扫描）。
    ///
    /// 返回 `RefCell` 守卫而非裸引用：缓存项会被 `borrow_mut` 改写，
    /// 逃逸引用会让后续可变借用指向同一内存。守卫保证同一次调用期间
    /// 缓存项不可被改写，调用方拿到的数据自洽。
    pub(crate) fn arrangement_selection_hits(&self) -> Ref<'_, ArrangeSelectionHits> {
        let editor_data = &self.editor_state.data;
        let key = (
            editor_data.track_notes_gen,
            editor_data.arrange_selection.revision(),
            visual_order_fingerprint(&editor_data.track_visual_order),
        );

        // 快路径：键命中直接返回（守卫在此随 return 转移，不提前 drop）
        {
            let cache = self.arrange_selection_cache.borrow();
            if let Some(hits) = cache.as_ref()
                && hits.key == key
            {
                return Ref::map(cache, |c| {
                    c.as_ref().expect("快路径已判定 Some，此处必然命中")
                });
            }
        }
        // 慢路径：上作用域的 `Ref` 已释放，这里才能可变借用

        let hits = Self::scan_arrangement_selection(editor_data, key);
        *self.arrange_selection_cache.borrow_mut() = Some(hits);
        Ref::map(self.arrange_selection_cache.borrow(), |c| {
            c.as_ref().expect("走带选区缓存刚写入即应命中")
        })
    }

    /// 全量扫描文档，按走带选区收集命中音符（唯一的 O(全文档音符) 入口）。
    fn scan_arrangement_selection(
        editor_data: &crate::editor_state::EditorData,
        key: (u64, u64, u64),
    ) -> ArrangeSelectionHits {
        let selection = &editor_data.arrange_selection;
        let Some(doc) = &editor_data.document else {
            return ArrangeSelectionHits {
                key,
                by_track: Vec::new(),
                flat: Rc::from(Vec::new()),
            };
        };
        // 空选区 O(1) 早退：不扫文档
        if selection.is_empty() {
            return ArrangeSelectionHits {
                key,
                by_track: Vec::new(),
                flat: Rc::from(Vec::new()),
            };
        }

        let track_count = doc.track_count();
        // 视觉位置映射**一次性建表**（O(音轨数)）：
        // 旧实现在轨道循环内调 `visual_position_of`（其内部是 O(音轨数) 线性
        // 查找），整体退化成 O(音轨数² + 全音符)。
        let mut doc_to_visual = vec![usize::MAX; track_count];
        for (visual_pos, &doc_idx) in editor_data.track_visual_order.iter().enumerate() {
            if doc_idx < track_count {
                doc_to_visual[doc_idx] = visual_pos;
            }
        }

        let mut by_track: Vec<(usize, Vec<NoteEvent>)> = Vec::new();
        let mut flat: Vec<(f64, f64, usize, u8)> = Vec::new();
        for (track_idx, &mapped) in doc_to_visual.iter().enumerate() {
            // 视觉位置映射未覆盖该轨时回退恒等（与 `visual_position_of` 的
            // `unwrap_or(track_idx)` 语义一致）
            let visual_pos = if mapped == usize::MAX {
                track_idx
            } else {
                mapped
            };
            let mut selected: Vec<NoteEvent> = Vec::new();
            for note_event in editor_data.track_notes(track_idx) {
                if selection.contains(visual_pos as u16, note_event.start_tick, note_event.key) {
                    selected.push(*note_event);
                    flat.push((
                        note_event.start_tick as f64,
                        note_event.end_tick as f64,
                        visual_pos,
                        note_event.key,
                    ));
                }
            }
            if !selected.is_empty() {
                by_track.push((track_idx, selected));
            }
        }

        ArrangeSelectionHits {
            key,
            by_track,
            flat: Rc::from(flat),
        }
    }

    /// 走带选区是否命中**任何**音符（零分配）。
    ///
    /// 判据与 [`crate::Editor::has_active_selection`] 的走带分支同源
    /// （同一份缓存），保证「菜单可用性」与「操作闸门」口径一致。
    pub fn arrangement_has_selected_notes(&self) -> bool {
        !self.arrangement_selection_hits().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::Note;
    use crate::tests::test_helpers::seed_notes;

    /// 2 轨 × 每轨 2 音符；走带选区框住两轨全部音符
    fn editor_with_selection() -> Editor {
        let mut editor = Editor::default();
        // `seed_notes` 会整体替换 document，故先建轨 0，再用 `insert_note` 追加轨 1
        seed_notes(
            &mut editor,
            2,
            0,
            &[
                Note::from_raw(100.0, 60, 100.0, 100, 0),
                Note::from_raw(300.0, 60, 100.0, 100, 0),
            ],
        );
        editor
            .editor_state
            .data
            .insert_note(1, Note::from_raw(100.0, 62, 100.0, 100, 0));
        editor
            .editor_state
            .data
            .insert_note(1, Note::from_raw(300.0, 62, 100.0, 100, 0));
        editor
            .editor_state
            .data
            .arrange_selection
            .add_rect_track(50, 450, 0, 127, 0, 1);
        editor
    }

    /// 基线：框选命中全部 4 个音符，`by_track` 与 `flat` 两种形态一致
    #[test]
    fn test_hits_cover_both_consumption_shapes() {
        let editor = editor_with_selection();
        let hits = editor.arrangement_selection_hits();
        assert_eq!(hits.by_track().len(), 2, "应命中 2 条音轨");
        assert_eq!(
            hits.by_track().iter().map(|(_, n)| n.len()).sum::<usize>(),
            4,
            "应命中 4 个音符"
        );
        assert_eq!(hits.flat().len(), 4, "扁平表应与分组表等长");
        assert!(!hits.is_empty());
    }

    /// 缓存键命中：连续两次查询返回**同一份** Rc（证明未重扫文档）
    #[test]
    fn test_cache_hit_reuses_same_allocation() {
        let editor = editor_with_selection();
        let first = editor.arrangement_selected_notes();
        let second = editor.arrangement_selected_notes();
        assert_eq!(first.len(), 4);
        assert!(
            std::rc::Rc::ptr_eq(&first, &second),
            "键未变时必须复用同一份缓存，view 层每帧克隆才只是引用计数自增"
        );
    }

    /// 选区变化 → 缓存失效并重算（revision 参与键）
    #[test]
    fn test_selection_change_invalidates_cache() {
        let mut editor = editor_with_selection();
        assert_eq!(editor.arrangement_selected_notes().len(), 4);

        // 平移选区到空白处 → 命中集合应变空
        editor
            .editor_state
            .data
            .arrange_selection
            .offset_ticks(10_000);
        assert!(
            editor.arrangement_selected_notes().is_empty(),
            "选区平移后必须重算，不能读到平移前的缓存"
        );
        assert!(!editor.arrangement_has_selected_notes());
    }

    /// 选区清空 → 缓存失效
    #[test]
    fn test_clear_selection_invalidates_cache() {
        let mut editor = editor_with_selection();
        assert!(editor.arrangement_has_selected_notes());
        editor.editor_state.data.arrange_selection.clear();
        assert!(
            !editor.arrangement_has_selected_notes(),
            "清空选区后不得继续报告有命中音符"
        );
    }

    /// 音符数据变化 → 缓存失效（track_notes_gen 参与键）
    #[test]
    fn test_note_data_change_invalidates_cache() {
        let mut editor = editor_with_selection();
        let before = editor.arrangement_selected_notes();
        assert_eq!(before.len(), 4);

        // 新增一个落在选区内的音符
        editor
            .editor_state
            .data
            .insert_note(0, Note::from_raw(200.0, 64, 10.0, 100, 0));
        let after = editor.arrangement_selected_notes();
        assert_eq!(
            after.len(),
            5,
            "音符数据变化后必须重扫，否则新增音符不会被选区命中"
        );
    }

    /// 侧边栏排序变化 → 缓存失效（视觉位置映射改变）
    #[test]
    fn test_visual_order_change_invalidates_cache() {
        let mut editor = editor_with_selection();
        // `add_rect_track` 是**并集追加**语义，先清空再框选视觉轨 0
        editor.editor_state.data.arrange_selection.clear();
        editor
            .editor_state
            .data
            .arrange_selection
            .add_rect_track(50, 450, 0, 127, 0, 0);
        let before = editor.arrangement_selected_notes();
        assert_eq!(before.len(), 2, "恒等映射下应命中文档轨 0 的 2 个音符");
        assert!(before.iter().all(|&(_, _, track, _)| track == 0));
        assert!(before.iter().all(|&(_, _, _, key)| key == 60));

        // 交换视觉顺序：文档轨 1 变成视觉轨 0
        editor.editor_state.data.track_visual_order = vec![1, 0];
        let after = editor.arrangement_selected_notes();
        assert_eq!(
            after.len(),
            2,
            "轨道序变化后必须重算（track_notes_gen 与选区都没变）"
        );
        assert!(
            after.iter().all(|&(_, _, track, _)| track == 0),
            "视觉轨 0 现在对应文档轨 1"
        );
        assert!(
            after.iter().all(|&(_, _, _, key)| key == 62),
            "命中的应是文档轨 1 的音符（key 62），证明映射已随排序切换"
        );
    }

    /// 冻结选区（`freeze`）→ 缓存失效
    #[test]
    fn test_freeze_invalidates_cache() {
        let mut editor = editor_with_selection();
        assert_eq!(editor.arrangement_selected_notes().len(), 4);
        editor
            .editor_state
            .data
            .arrange_selection
            .freeze([(0u16, 100u32, 200u32, 60u8)]);
        assert_eq!(
            editor.arrangement_selected_notes().len(),
            1,
            "冻结为精确集合后只应命中 1 个音符"
        );
    }

    /// 空选区：O(1) 早退且不产生任何命中
    #[test]
    fn test_empty_selection_yields_no_hits() {
        let editor = Editor::default();
        assert!(!editor.arrangement_has_selected_notes());
        assert!(editor.arrangement_selected_notes().is_empty());
    }
}
