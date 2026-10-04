//! 已确认（√）绘制图形的对象注册表、选中态与编辑态
//!
//! 曲线 / 形状 / 画刷工具的原范式是「绘制 → √ 生成音符 → **丢弃矢量几何**」，
//! 因此确认之后无法再指认「刚才画的是哪一个图形」。本模块引入**图形对象**：
//! √ 确认时把矢量几何 + 它生成的音符登记进注册表（音符仍是真正的文档内容，
//! 几何只是叠加对象），供「鼠标工具」（`Tool::ShapeSelect`）点选、移动、删除。
//!
//! ## 与历史（撤销/重做）的关系
//!
//! 图形与它的音符同生共死，靠**历史分组 ID**绑定（见 `Editor::undo/redo` →
//! `on_undo_group` / `on_redo_group`）：
//! - `group`（创建分组）：撤销该创建 → `hidden_by_creation = true`（图形+音符都没了）；
//! - `delete_group`（删除分组）：重做该删除 → `deleted = true`；
//! - `moves[].group`（移动分组）：撤销该移动 → 几何反向平移回去。
//!
//! ## 生命周期
//!
//! 注册表不参与文档序列化（不是文档内容），文档重建（`EditorState::reset`）时清空；
//! 切换工具**不**清空（选中是跨工具的持久对象状态）。

use std::collections::HashSet;

use super::shape_tool::ShapeKind;

/// 某个图形 √ 确认时生成的**单个音符**（逻辑值，用于整体移动 / 删除）
///
/// 只记录定位所需的最小字段：`(track, tick, key, length)`——与文档里
/// `NoteEvent` 的值语义一致，可经 `position_of_unused` 按值精确定位。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeNote {
    /// 音符所在音轨
    pub track: usize,
    /// 起始 tick
    pub tick: f32,
    /// 音高
    pub key: u16,
    /// 时长（tick）
    pub length: f32,
}

impl ShapeNote {
    /// 平移出新音符（tick / key 位移，长度不变）
    pub fn translated(&self, dtick: f32, dkey: f32) -> Self {
        Self {
            track: self.track,
            tick: self.tick + dtick,
            key: (self.key as f32 + dkey).round().clamp(0.0, 255.0) as u16,
            length: self.length,
        }
    }
}

/// 已确认绘制图形的矢量几何来源
#[derive(Debug, Clone, PartialEq)]
pub enum DrawnShapeSource {
    /// 形状工具：矩形 / 圆 / 三角（外接框 + 类型 + Shift 约束 + 是否填充）
    Shape {
        /// 图形类型
        kind: ShapeKind,
        /// 外接框逻辑坐标 (tick_lo, key_lo, tick_hi, key_hi)
        rect: (f32, f32, f32, f32),
        /// 绘制时是否按住 Shift（屏幕空间正图形约束）
        shift_constrained: bool,
        /// 是否填充内部（仅作信息保留，供后续编辑回放）
        filled: bool,
    },
    /// 折线（曲线工具展平路径 / 画刷笔画），逻辑坐标 (tick, key)
    Polyline {
        /// 折线点列（至少 1 个点）
        points: Vec<(f32, f32)>,
    },
}

impl DrawnShapeSource {
    /// 整体平移几何（tick 自由、key 取整后钳到 0..=255）
    pub fn translate(&mut self, dtick: f32, dkey: f32) {
        match self {
            Self::Shape { rect, .. } => {
                rect.0 = (rect.0 + dtick).max(0.0);
                rect.2 = (rect.2 + dtick).max(0.0);
                rect.1 = (rect.1 + dkey).clamp(0.0, 255.0);
                rect.3 = (rect.3 + dkey).clamp(0.0, 255.0);
            }
            Self::Polyline { points } => {
                for p in points.iter_mut() {
                    p.0 = (p.0 + dtick).max(0.0);
                    p.1 = (p.1 + dkey).clamp(0.0, 255.0);
                }
            }
        }
    }

    /// 外接框（逻辑坐标 (min_tick, max_tick, min_key, max_key)）；空折线返回 `None`
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        match self {
            Self::Shape { rect, .. } => {
                let (x0, y0, x1, y1) = *rect;
                Some((x0.min(x1), x0.max(x1), y0.min(y1), y0.max(y1)))
            }
            Self::Polyline { points } => {
                let mut it = points.iter().copied();
                let first = it.next()?;
                let mut b = (first.0, first.0, first.1, first.1);
                for (t, k) in it {
                    b.0 = b.0.min(t);
                    b.1 = b.1.max(t);
                    b.2 = b.2.min(k);
                    b.3 = b.3.max(k);
                }
                Some(b)
            }
        }
    }
}

/// 已应用到某图形的一次整体移动（绑定历史分组，供撤销/重做反向回放）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeMove {
    /// 移动操作的历史分组 ID
    pub group: u64,
    /// tick 偏移
    pub dtick: f32,
    /// key 偏移
    pub dkey: f32,
}

/// 一个已确认的绘制图形对象
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnShape {
    /// 稳定标识（注册表内自增，选中态以此引用）
    pub id: u64,
    /// 所属音轨（仅在当前轨可点选 / 高亮 / 编辑）
    pub track: usize,
    /// 创建该图形的音符历史分组 ID（`None` = 未接入历史）
    pub group: Option<u64>,
    /// 矢量几何
    pub source: DrawnShapeSource,
    /// 该图形 √ 确认时生成的音符（整体移动 / 删除的作用对象）
    pub notes: Vec<ShapeNote>,
    /// 创建操作被撤销而隐藏
    pub hidden_by_creation: bool,
    /// 已被「删除图形」操作删除
    pub deleted: bool,
    /// 曾经被删除过（决定是否需要参与「音符是否还在」的对账；
    /// 未被本功能删除过的图形不参与，避免误隐藏用户手动擦除的音符所对应的轮廓）
    pub ever_deleted: bool,
    /// 已应用的移动（历史分组 + 偏移）
    pub moves: Vec<ShapeMove>,
}

impl DrawnShape {
    /// 当前是否可见（未被撤销创建、未被删除）
    pub fn is_visible(&self) -> bool {
        !self.hidden_by_creation && !self.deleted
    }
}

/// 图形拖拽移动状态（鼠标工具按住选中图形拖动）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeDrag {
    /// 被拖拽的图形 ID
    pub shape_id: u64,
    /// 按下时的逻辑坐标 (tick, key)
    pub start_tick: f32,
    /// 按下时的逻辑 key
    pub start_key: f32,
    /// 当前 tick 偏移
    pub delta_tick: f32,
    /// 当前 key 偏移
    pub delta_key: f32,
}

/// 空白处拉框（框选）中的矩形，逻辑坐标 (tick, key)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeMarquee {
    /// 按下点 tick
    pub start_tick: f32,
    /// 按下点 key
    pub start_key: f32,
    /// 当前点 tick
    pub cur_tick: f32,
    /// 当前点 key
    pub cur_key: f32,
}

impl ShapeMarquee {
    /// 规范化逻辑矩形 `(min_tick, max_tick, min_key, max_key)`
    ///
    /// 与 [`DrawnShapeSource::bounds`] 同布局，相交判定可直接比较。
    pub fn rect(&self) -> (f32, f32, f32, f32) {
        (
            self.start_tick.min(self.cur_tick),
            self.start_tick.max(self.cur_tick),
            self.start_key.min(self.cur_key),
            self.start_key.max(self.cur_key),
        )
    }
}

/// 图形选中状态
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShapeSelectState {
    /// 全部已登记图形（含被隐藏 / 已删除的）
    shapes: Vec<DrawnShape>,
    /// 下一个自增 ID
    next_id: u64,
    /// 当前选中的图形 ID
    selected: Option<u64>,
    /// 拖拽移动状态
    drag: Option<ShapeDrag>,
    /// 空白处拉框（框选）状态
    marquee: Option<ShapeMarquee>,
}

impl ShapeSelectState {
    /// 清空全部图形与状态（文档重建时调用）
    pub fn clear(&mut self) {
        self.shapes.clear();
        self.next_id = 0;
        self.selected = None;
        self.drag = None;
        self.marquee = None;
    }

    /// 登记一个已确认图形（几何 + 它生成的音符），返回其稳定 ID
    pub fn add(
        &mut self,
        track: usize,
        group: Option<u64>,
        source: DrawnShapeSource,
        notes: Vec<ShapeNote>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.shapes.push(DrawnShape {
            id,
            track,
            group,
            source,
            notes,
            hidden_by_creation: false,
            deleted: false,
            ever_deleted: false,
            moves: Vec::new(),
        });
        id
    }

    /// 全部图形（含隐藏 / 已删除）
    pub fn shapes(&self) -> &[DrawnShape] {
        &self.shapes
    }

    /// 图形总数
    pub fn len(&self) -> usize {
        self.shapes.len()
    }

    /// 是否无任何图形
    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    /// 当前选中的图形 ID
    pub fn selected(&self) -> Option<u64> {
        self.selected
    }

    /// 选中指定图形；`None` = 取消选中。指向不存在 / 不可见的 ID 时退化为取消选中。
    pub fn select(&mut self, id: Option<u64>) {
        self.selected = match id {
            Some(id)
                if self
                    .shapes
                    .iter()
                    .any(|s| s.id == id && s.is_visible()) =>
            {
                Some(id)
            }
            _ => None,
        };
    }

    /// 当前选中的图形对象（仅在可见时返回）
    pub fn selected_shape(&self) -> Option<&DrawnShape> {
        self.shapes.iter().find(|s| {
            Some(s.id) == self.selected && s.is_visible()
        })
    }

    /// 当前选中的图形对象（可变）
    pub fn selected_shape_mut(&mut self) -> Option<&mut DrawnShape> {
        let id = self.selected?;
        self.shapes.iter_mut().find(|s| s.id == id && s.is_visible())
    }

    /// 某音轨上当前可见的图形，按登记顺序
    pub fn visible_on(&self, track: usize) -> impl DoubleEndedIterator<Item = &DrawnShape> {
        self.shapes
            .iter()
            .filter(move |s| s.track == track && s.is_visible())
    }

    /// 历史分组被撤销：撤销创建 → 隐藏该图形；撤销移动 → 几何反向平移
    ///
    /// **删除状态不在此处理**：删除走的是 Snapshot 历史（撤销时 redo 侧的快照
    /// 不携带分组 ID，无法对称回放），改由 [`Self::reconcile_alive`] 按
    /// 「该图形的音符是否还在文档里」对账，方向无关、天然对称。
    pub fn on_undo_group(&mut self, group: u64) {
        for s in &mut self.shapes {
            if s.group == Some(group) {
                s.hidden_by_creation = true;
            }
            // 撤销一次移动 → 几何反向平移回去
            if let Some(mv) = s.moves.iter().find(|m| m.group == group).copied() {
                s.source.translate(-mv.dtick, -mv.dkey);
                for n in &mut s.notes {
                    *n = n.translated(-mv.dtick, -mv.dkey);
                }
            }
        }
        self.collapse_selection();
    }

    /// 历史分组被重做：重做创建 → 恢复该图形；重做移动 → 几何正向平移
    pub fn on_redo_group(&mut self, group: u64) {
        for s in &mut self.shapes {
            if s.group == Some(group) {
                s.hidden_by_creation = false;
            }
            if let Some(mv) = s.moves.iter().find(|m| m.group == group).copied() {
                s.source.translate(mv.dtick, mv.dkey);
                for n in &mut s.notes {
                    *n = n.translated(mv.dtick, mv.dkey);
                }
            }
        }
        self.collapse_selection();
    }

    /// 标记图形为「已删除」（音符已由调用方从文档删除）
    pub fn mark_deleted(&mut self, id: u64) -> bool {
        let Some(s) = self.shapes.iter_mut().find(|s| s.id == id) else {
            return false;
        };
        s.deleted = true;
        s.ever_deleted = true;
        self.collapse_selection();
        true
    }

    /// 与文档对账：按「该图形是否还有音符残留」修正 `deleted`
    ///
    /// `alive` 为 `(图形 ID, 是否还有音符)` 列表（由调用方查文档得出）。
    /// 只对 `ever_deleted` 的图形生效——未被本功能删除过的图形不参与，
    /// 避免误隐藏「用户手动擦除音符但保留轮廓」的场景。
    /// 撤销/重做后调用，使删除状态天然对称（不依赖历史分组 ID）。
    pub fn reconcile_alive(&mut self, alive: &[(u64, bool)]) {
        for (id, ok) in alive {
            if let Some(s) = self.shapes.iter_mut().find(|s| s.id == *id)
                && s.ever_deleted
                && !s.notes.is_empty()
            {
                s.deleted = !*ok;
            }
        }
        self.collapse_selection();
    }

    /// 记录一次已应用的移动（绑定历史分组），并把几何 / 音符平移到位
    pub fn translate_shape(&mut self, id: u64, group: Option<u64>, dtick: f32, dkey: f32) -> bool {
        let Some(s) = self.shapes.iter_mut().find(|s| s.id == id) else {
            return false;
        };
        s.source.translate(dtick, dkey);
        for n in &mut s.notes {
            *n = n.translated(dtick, dkey);
        }
        if let Some(g) = group {
            s.moves.push(ShapeMove {
                group: g,
                dtick,
                dkey,
            });
        }
        true
    }

    // ── 拖拽移动 ─────────────────────────────────────────

    /// 开始拖拽指定图形
    pub fn begin_drag(&mut self, shape_id: u64, start_tick: f32, start_key: f32) {
        self.drag = Some(ShapeDrag {
            shape_id,
            start_tick,
            start_key,
            delta_tick: 0.0,
            delta_key: 0.0,
        });
    }

    /// 更新拖拽偏移（相对按下点）
    pub fn update_drag(&mut self, tick: f32, key: f32, snap: f32) {
        let Some(d) = self.drag.as_mut() else {
            return;
        };
        let snap = snap.max(1.0);
        let raw_dtick = tick - d.start_tick;
        // tick 对齐到吸附网格（与音符移动一致，保证落点干净）；key 取整
        d.delta_tick = (raw_dtick / snap).round() * snap;
        d.delta_key = (key - d.start_key).round();
    }

    /// 当前拖拽状态
    pub fn drag(&self) -> Option<ShapeDrag> {
        self.drag
    }

    /// 是否正在拖拽图形
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// 结束拖拽并返回 `(图形 ID, dtick, dkey)`；无实际位移时返回 `None`
    pub fn end_drag(&mut self) -> Option<(u64, f32, f32)> {
        let d = self.drag.take()?;
        if d.delta_tick == 0.0 && d.delta_key == 0.0 {
            return None;
        }
        Some((d.shape_id, d.delta_tick, d.delta_key))
    }

    // ── 空白拉框（框选） ───────────────────────────────────

    /// 从空白处起框（框选）
    pub fn begin_marquee(&mut self, tick: f32, key: f32) {
        self.marquee = Some(ShapeMarquee {
            start_tick: tick,
            start_key: key,
            cur_tick: tick,
            cur_key: key,
        });
    }

    /// 更新拉框当前点
    pub fn update_marquee(&mut self, tick: f32, key: f32) {
        if let Some(m) = self.marquee.as_mut() {
            m.cur_tick = tick;
            m.cur_key = key;
        }
    }

    /// 当前拉框矩形（供叠加层绘制）
    pub fn marquee(&self) -> Option<ShapeMarquee> {
        self.marquee
    }

    /// 是否正在拉框
    pub fn is_marqueeing(&self) -> bool {
        self.marquee.is_some()
    }

    /// 结束拉框并返回其矩形（供命中判定）
    pub fn end_marquee(&mut self) -> Option<ShapeMarquee> {
        self.marquee.take()
    }

    /// 删除指定图形记录（含选中态收敛）
    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.shapes.len();
        self.shapes.retain(|s| s.id != id);
        if self.selected == Some(id) {
            self.selected = None;
        }
        self.shapes.len() != before
    }

    /// 收敛选中态：选中项不可见 / 不存在时取消选中
    fn collapse_selection(&mut self) {
        if let Some(id) = self.selected
            && !self.shapes.iter().any(|s| s.id == id && s.is_visible())
        {
            self.selected = None;
        }
    }

    /// 当前可见图形引用的历史分组集合（供调试 / 测试）
    pub fn active_groups(&self) -> HashSet<u64> {
        self.shapes
            .iter()
            .filter_map(|s| if s.is_visible() { s.group } else { None })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_source() -> DrawnShapeSource {
        DrawnShapeSource::Shape {
            kind: ShapeKind::Rectangle,
            rect: (0.0, 60.0, 4.0, 64.0),
            shift_constrained: false,
            filled: false,
        }
    }

    fn note(tick: f32, key: u16) -> ShapeNote {
        ShapeNote {
            track: 1,
            tick,
            key,
            length: 1.0,
        }
    }

    #[test]
    fn test_add_assigns_stable_ids_and_notes() {
        let mut st = ShapeSelectState::default();
        let a = st.add(1, Some(7), rect_source(), vec![note(0.0, 60)]);
        let b = st.add(1, None, rect_source(), vec![]);
        assert_ne!(a, b, "不同图形必须得到不同 ID");
        assert_eq!(st.len(), 2);
        assert_eq!(st.shapes()[0].notes.len(), 1);
    }

    #[test]
    fn test_select_validates_existence_and_visibility() {
        let mut st = ShapeSelectState::default();
        let a = st.add(1, Some(10), rect_source(), vec![]);
        st.select(Some(a));
        assert_eq!(st.selected(), Some(a));
        st.select(Some(9999));
        assert_eq!(st.selected(), None);
        // 撤销创建后不可再选中
        st.select(Some(a));
        st.on_undo_group(10);
        assert_eq!(st.selected(), None, "被隐藏的图形不应保持选中");
    }

    #[test]
    fn test_visible_on_filters_track_and_state() {
        let mut st = ShapeSelectState::default();
        st.add(1, Some(10), rect_source(), vec![]);
        st.add(2, Some(10), rect_source(), vec![]);
        st.add(1, Some(20), rect_source(), vec![]);
        assert_eq!(st.visible_on(1).count(), 2);
        st.on_undo_group(10);
        assert_eq!(st.visible_on(1).count(), 1);
        assert_eq!(st.visible_on(2).count(), 0);
        st.on_redo_group(10);
        assert_eq!(st.visible_on(1).count(), 2);
    }

    #[test]
    fn test_delete_and_reconcile_restores() {
        let mut st = ShapeSelectState::default();
        let a = st.add(1, Some(1), rect_source(), vec![note(0.0, 60)]);
        st.select(Some(a));
        assert!(st.mark_deleted(a));
        assert_eq!(st.visible_on(1).count(), 0, "删除后不可见");
        assert_eq!(st.selected(), None, "删除应收敛选中");
        // 撤销删除 → 音符回来了 → 对账恢复可见
        st.reconcile_alive(&[(a, true)]);
        assert_eq!(st.visible_on(1).count(), 1, "撤销删除应恢复");
        // 重做删除 → 音符又没了
        st.reconcile_alive(&[(a, false)]);
        assert_eq!(st.visible_on(1).count(), 0, "重做删除应再次隐藏");
    }

    #[test]
    fn test_reconcile_ignores_never_deleted_shapes() {
        let mut st = ShapeSelectState::default();
        let a = st.add(1, None, rect_source(), vec![note(0.0, 60)]);
        // 未走过删除流程的图形：即使音符不在，也不因对账被隐藏（避免误伤手动擦除场景）
        st.reconcile_alive(&[(a, false)]);
        assert_eq!(st.visible_on(1).count(), 1);
        // 空音符图形同样不参与对账
        let b = st.add(1, None, rect_source(), vec![]);
        st.mark_deleted(b);
        st.reconcile_alive(&[(b, false)]);
        assert_eq!(st.visible_on(1).count(), 1);
    }

    #[test]
    fn test_translate_updates_geometry_and_notes_and_undoes() {
        let mut st = ShapeSelectState::default();
        let a = st.add(1, None, rect_source(), vec![note(10.0, 60)]);
        assert!(st.translate_shape(a, Some(5), 100.0, 2.0));
        let s = &st.shapes()[0];
        match &s.source {
            DrawnShapeSource::Shape { rect, .. } => assert_eq!(*rect, (100.0, 62.0, 104.0, 66.0)),
            other => panic!("期望 Shape，实际 {other:?}"),
        }
        assert_eq!(s.notes[0].tick, 110.0);
        assert_eq!(s.notes[0].key, 62);
        st.on_undo_group(5);
        let s = &st.shapes()[0];
        match &s.source {
            DrawnShapeSource::Shape { rect, .. } => assert_eq!(*rect, (0.0, 60.0, 4.0, 64.0)),
            other => panic!("期望 Shape，实际 {other:?}"),
        }
        assert_eq!(s.notes[0].tick, 10.0);
        assert_eq!(s.notes[0].key, 60);
        st.on_redo_group(5);
        assert_eq!(st.shapes()[0].notes[0].tick, 110.0);
    }

    #[test]
    fn test_drag_lifecycle() {
        let mut st = ShapeSelectState::default();
        let a = st.add(1, None, rect_source(), vec![]);
        st.begin_drag(a, 0.0, 60.0);
        assert!(st.is_dragging());
        // snap = 10：raw 6 → 对齐到 10；raw 14 → 10
        st.update_drag(6.0, 62.4, 10.0);
        let d = st.drag().expect("拖拽态应存在");
        assert_eq!(d.delta_tick, 10.0);
        assert_eq!(d.delta_key, 2.0);
        st.update_drag(14.0, 62.4, 10.0);
        assert_eq!(st.drag().expect("拖拽态应存在").delta_tick, 10.0);
        let out = st.end_drag();
        assert_eq!(out, Some((a, 10.0, 2.0)));
        assert!(!st.is_dragging());
    }

    #[test]
    fn test_drag_without_movement_returns_none() {
        let mut st = ShapeSelectState::default();
        let a = st.add(1, None, rect_source(), vec![]);
        st.begin_drag(a, 0.0, 60.0);
        st.update_drag(0.4, 60.0, 1.0);
        assert_eq!(st.end_drag(), None, "原地按一下不应产生移动历史");
    }

    #[test]
    fn test_bounds_for_shape_and_polyline() {
        let s = DrawnShapeSource::Shape {
            kind: ShapeKind::Rectangle,
            rect: (4.0, 64.0, 0.0, 60.0),
            shift_constrained: false,
            filled: false,
        };
        assert_eq!(s.bounds(), Some((0.0, 4.0, 60.0, 64.0)));
        let p = DrawnShapeSource::Polyline {
            points: vec![(5.0, 61.0), (1.0, 70.0)],
        };
        assert_eq!(p.bounds(), Some((1.0, 5.0, 61.0, 70.0)));
        assert_eq!(
            DrawnShapeSource::Polyline { points: vec![] }.bounds(),
            None
        );
    }

    #[test]
    fn test_marquee_lifecycle_normalizes_rect() {
        let mut st = ShapeSelectState::default();
        assert!(!st.is_marqueeing());
        st.begin_marquee(10.0, 70.0);
        assert!(st.is_marqueeing());
        // 反向拖动（左下 → 右上）应被规范化成 (min, max) 布局
        st.update_marquee(2.0, 60.0);
        let m = st.marquee().expect("拉框态应存在");
        assert_eq!(m.rect(), (2.0, 10.0, 60.0, 70.0));
        let taken = st.end_marquee().expect("结束应返回矩形");
        assert_eq!(taken.rect(), (2.0, 10.0, 60.0, 70.0));
        assert!(!st.is_marqueeing(), "结束后拉框态应清空");
        assert!(st.end_marquee().is_none(), "重复结束应为 None");
    }

    #[test]
    fn test_clear_resets_marquee() {
        let mut st = ShapeSelectState::default();
        st.add(1, None, rect_source(), vec![]);
        st.begin_marquee(0.0, 60.0);
        st.clear();
        assert!(!st.is_marqueeing());
        assert!(st.is_empty());
    }

    #[test]
    fn test_clear_resets_everything() {
        let mut st = ShapeSelectState::default();
        let a = st.add(1, Some(3), rect_source(), vec![]);
        st.select(Some(a));
        st.clear();
        assert!(st.is_empty());
        assert_eq!(st.selected(), None);
        assert!(!st.is_dragging());
        assert_eq!(st.add(1, None, rect_source(), vec![]), 0);
    }
}
