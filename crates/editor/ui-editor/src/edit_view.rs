//! 编辑视图仲裁（钢琴卷帘 ⇄ 工程走带的单一权威源）
//!
//! # 为什么需要这个模块
//!
//! 两个编辑视图各有**彼此独立、互不清理**的选区：
//! - 钢琴卷帘：`interaction.selected_notes`（`SelectionSet`，**单轨**音符索引位图）
//! - 工程走带：`data.arrange_selection`（`ArrangeSelection`，**跨轨**矩形 + 冻结集）
//!
//! 而「用户当前在编辑哪个视图」这个事实此前由各调用点**自行判定**，散落 5 处、
//! 两种写法（`Root::is_arrangement_mode()` 与裸比 `sidebar.route ==
//! Route::Arrangement` 混用）。后果是每个新编辑命令都可能：
//! - 忘记判视图 → 在走带视图作用到卷帘选区（曾致「点量化静默量化整轨」）
//! - 抄错写法 → 判到错误的选区
//!
//! 本模块把两件事收敛成**唯一入口**：
//! 1. [`EditView`]：视图枚举。视图**只能**由 `Root::edit_view()` 从侧栏路由派生，
//!    业务代码不得自行判断。
//! 2. [`SelectionSnapshot`]：视图无关的选中音符快照。选区解析只有一份实现，
//!    调用方**无法**绕过视图直接读某个选区。
//!
//! # 边界：渲染层不收口
//!
//! 渲染层（`host/render/*`）也判定 `is_arrangement_mode()`，但那里的语义是
//! 「**当前渲染布局与数据源**是走带」（viewport 参数、走带覆盖层、泳道可见性、
//! CC 面板跳过等），与「**编辑命令作用于哪套选区**」是不同关注点。强行合并会让
//! `EditView` 承担渲染语义、职责膨胀。渲染层保持独立判定，但**统一写法**
//!（一律走 `Root::is_arrangement_mode()`，不再裸比 `sidebar.route`）。

use lumino_midi_loader::NoteEvent;

/// 编辑视图：钢琴卷帘 / 工程走带
///
/// 选区归属由此决定；新增编辑视图时在此扩展，所有 `match` 都会因穷尽性检查
/// 而**编译失败**——这正是「不可能忘记判视图」的机制保障。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EditView {
    /// 钢琴卷帘（单轨编辑，选区为当前轨音符索引位图）
    ///
    /// 默认视图：应用启动与绝大多数编辑场景都在卷帘。
    #[default]
    PianoRoll,
    /// 工程走带（跨轨编辑，选区为跨轨矩形 / 冻结精确集）
    Arrangement,
}

impl EditView {
    /// 是否为工程走带
    #[inline]
    pub fn is_arrangement(self) -> bool {
        matches!(self, Self::Arrangement)
    }

    /// 该视图的选区是否天然跨轨
    ///
    /// 卷帘选区是单轨索引位图，无法表达跨轨选择——批量操作需先确认作用域。
    #[inline]
    pub fn selection_is_cross_track(self) -> bool {
        self.is_arrangement()
    }

    /// 视图标识（用于日志与诊断，避免裸枚举 Debug 输出）
    #[inline]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PianoRoll => "钢琴卷帘",
            Self::Arrangement => "工程走带",
        }
    }
}

/// 视图无关的选中音符快照：按**文档音轨**分组的音符值
///
/// 由 [`crate::Editor::resolve_selection`] 构造。设计要点：
/// - **只读值**（`NoteEvent` 按值复制），不持有 `&mut` / 不暴露选区内部结构；
/// - 附带产出它的 [`EditView`]，调用方可据此决定后续行为（如粘贴落点）；
/// - 判空语义统一为「没有任何命中音符」，与菜单项启用条件对齐。
#[derive(Debug, Clone, Default)]
pub struct SelectionSnapshot {
    view: EditView,
    tracks: Vec<(usize, Vec<NoteEvent>)>,
}

impl SelectionSnapshot {
    /// 构造空快照（指定视图）
    pub fn empty(view: EditView) -> Self {
        Self {
            view,
            tracks: Vec::new(),
        }
    }

    /// 构造快照
    pub fn new(view: EditView, tracks: Vec<(usize, Vec<NoteEvent>)>) -> Self {
        Self { view, tracks }
    }

    /// 产出该快照的视图
    #[inline]
    pub fn view(&self) -> EditView {
        self.view
    }

    /// 是否没有任何命中音符
    ///
    /// 注意：判据是「**命中音符数**」而非「选区结构非空」。两套选区都可能
    /// 「结构非空但一个音符都没命中」（如框选落在空白处），而菜单项的可用性
    /// 必须以实际能操作的对象为准——否则会出现「能点却什么都不做」。
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.tracks.iter().all(|(_, notes)| notes.is_empty())
    }

    /// 命中音符总数（跨轨合计）
    pub fn note_count(&self) -> usize {
        self.tracks.iter().map(|(_, notes)| notes.len()).sum()
    }

    /// 命中音符所在的音轨数
    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }

    /// 按文档音轨分组的命中音符（仅含非空轨）
    #[inline]
    pub fn tracks(&self) -> &[(usize, Vec<NoteEvent>)] {
        &self.tracks
    }

    /// 消费快照，取出分组结果
    #[inline]
    pub fn into_tracks(self) -> Vec<(usize, Vec<NoteEvent>)> {
        self.tracks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_view_predicates() {
        assert!(EditView::Arrangement.is_arrangement());
        assert!(!EditView::PianoRoll.is_arrangement());
        assert!(EditView::Arrangement.selection_is_cross_track());
        assert!(!EditView::PianoRoll.selection_is_cross_track());
        assert_eq!(EditView::PianoRoll.as_str(), "钢琴卷帘");
    }

    /// 空快照：两个视图都应判空（闸门与菜单可用性依赖此语义）
    #[test]
    fn test_empty_snapshot_is_empty_for_both_views() {
        for view in [EditView::PianoRoll, EditView::Arrangement] {
            let s = SelectionSnapshot::empty(view);
            assert!(s.is_empty(), "{view:?} 的空快照应判空");
            assert_eq!(s.note_count(), 0);
            assert_eq!(s.track_count(), 0);
            assert_eq!(s.view(), view, "空快照应保留视图信息");
        }
    }

    /// 含空轨分组时仍应判空——菜单可用性以「实际能操作的对象」为准
    #[test]
    fn test_snapshot_with_empty_track_groups_is_empty() {
        let s = SelectionSnapshot::new(EditView::Arrangement, vec![(0, Vec::new())]);
        assert!(
            s.is_empty(),
            "结构非空但无命中音符时必须判空，否则菜单能点却无对象可操作"
        );
        assert_eq!(s.track_count(), 1, "分组数仍应如实报告");
    }

    #[test]
    fn test_snapshot_counts() {
        let n = |tick, key| NoteEvent::new(tick, tick + 10, key, 100, 0);
        let s = SelectionSnapshot::new(
            EditView::Arrangement,
            vec![(0, vec![n(0, 60), n(10, 62)]), (2, vec![n(20, 64)])],
        );
        assert!(!s.is_empty());
        assert_eq!(s.note_count(), 3);
        assert_eq!(s.track_count(), 2);
    }
}
