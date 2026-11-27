//! 选择集合：哪些形状被选中、按轨道可见性与选择包围盒。
//!
//! 结构体字段仍在 `drawn_shape.rs`，这里只放方法。

use super::super::*;

impl ShapeSelectState {
    /// 清空全部图形与状态（文档重建时调用）
    pub fn clear(&mut self) {
        self.shapes.clear();
        self.next_id = 0;
        self.selected.clear();
        self.drag = None;
        self.marquee = None;
        self.restore.clear();
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

    // ── 选中集（多选） ─────────────────────────────────────

    /// 未过期的可见图形（内部查询）
    fn visible_shape(&self, id: u64) -> Option<&DrawnShape> {
        self.shapes.iter().find(|s| s.id == id && s.is_visible())
    }

    /// 选中集合（图形 ID，按加入顺序）
    pub fn selected_ids(&self) -> &[u64] {
        &self.selected
    }

    /// 选中数量
    pub fn selection_len(&self) -> usize {
        self.selected.len()
    }

    /// 某图形是否已被选中
    pub fn is_selected(&self, id: u64) -> bool {
        self.selected.contains(&id)
    }

    /// **唯一**选中一个图形（替换整个选中集）；指向不存在 / 不可见的 ID 时清空选中
    pub fn select_only(&mut self, id: u64) {
        self.selected.clear();
        if self.visible_shape(id).is_some() {
            self.selected.push(id);
        }
    }

    /// 用给定集合**替换**选中集（自动过滤不存在 / 不可见者并去重，保留给定顺序）
    pub fn select_all_of(&mut self, ids: impl IntoIterator<Item = u64>) {
        self.selected.clear();
        for id in ids {
            if !self.selected.contains(&id) && self.visible_shape(id).is_some() {
                self.selected.push(id);
            }
        }
    }

    /// 加入选中集（已选中 / 不存在 / 不可见时无变化）；返回是否发生变化
    pub fn add_to_selection(&mut self, id: u64) -> bool {
        if self.selected.contains(&id) || self.visible_shape(id).is_none() {
            return false;
        }
        self.selected.push(id);
        true
    }

    /// 从选中集移除；返回是否发生变化
    pub fn remove_from_selection(&mut self, id: u64) -> bool {
        let before = self.selected.len();
        self.selected.retain(|&x| x != id);
        self.selected.len() != before
    }

    /// 切换选中（返回切换后**是否处于选中态**）
    pub fn toggle_selection(&mut self, id: u64) -> bool {
        if self.is_selected(id) {
            self.remove_from_selection(id);
            false
        } else {
            self.add_to_selection(id);
            self.is_selected(id)
        }
    }

    /// 清空选中集
    pub fn clear_selection(&mut self) {
        self.selected.clear();
    }

    /// 选中集合内的可见图形（按**登记顺序**，便于渲染/遍历确定）
    pub fn selected_shapes(&self) -> impl Iterator<Item = &DrawnShape> {
        let selected = &self.selected;
        self.shapes
            .iter()
            .filter(move |s| s.is_visible() && selected.contains(&s.id))
    }

    /// 指定音轨上被选中的可见图形（按登记顺序）
    pub fn selected_shapes_on(&self, track: usize) -> impl Iterator<Item = &DrawnShape> {
        let selected = &self.selected;
        self.shapes
            .iter()
            .filter(move |s| Self::in_track(s, track) && s.is_visible() && selected.contains(&s.id))
    }

    /// 指定音轨上选中图形的**并集外接框** `(min_tick, max_tick, min_key, max_key)`
    ///
    /// UI 层以此换算「选中选框」（框选框 + 框内拖动命中区）：多选时是整组的外接框。
    /// 该轨无选中 / 全部几何为空时返回 `None`。
    pub fn selection_bounds_on(&self, track: usize) -> Option<(f32, f32, f32, f32)> {
        let mut acc: Option<(f32, f32, f32, f32)> = None;
        for s in self.selected_shapes_on(track) {
            let Some(b) = s.source.bounds() else {
                continue;
            };
            acc = Some(match acc {
                None => b,
                Some(a) => (a.0.min(b.0), a.1.max(b.1), a.2.min(b.2), a.3.max(b.3)),
            });
        }
        acc
    }

    /// 某音轨上当前可见的图形，按登记顺序
    pub fn visible_on(&self, track: usize) -> impl DoubleEndedIterator<Item = &DrawnShape> {
        self.shapes
            .iter()
            .filter(move |s| Self::in_track(s, track) && s.is_visible())
    }

    /// 收敛选中集：剔除已不可见 / 已不存在的图形
    pub(super) fn collapse_selection(&mut self) {
        let alive: Vec<u64> = self
            .shapes
            .iter()
            .filter(|s| s.is_visible())
            .map(|s| s.id)
            .collect();
        self.selected.retain(|id| alive.contains(id));
    }
}
