//! 已确认（√）绘制图形的对象注册表与选中状态
//!
//! 曲线 / 形状 / 画刷工具的原范式是「绘制 → √ 生成音符 → **丢弃矢量几何**」，
//! 因此确认之后无法再指认「刚才画的是哪一个图形」。本模块引入**图形对象**：
//! √ 确认时把矢量几何登记进注册表（音符仍然是真正的文档内容，几何只是叠加对象），
//! 供「鼠标工具」（`Tool::ShapeSelect`）点选，并以高亮描边显示。
//!
//! ## 与历史（撤销/重做）的关系
//!
//! 图形对象与音符创建同生共死：登记时记录音符创建历史条目的 `group_id`
//! （见 `Editor::undo/redo`），撤销该条目时把同组图形整体隐藏（`hide_group`），
//! 重做时恢复（`show_group`）——避免出现「描边还在、音符已没」的幽灵。
//!
//! ## 生命周期
//!
//! 注册表不参与文档序列化（不是文档内容），文档重建（`EditorState::reset`）时清空；
//! 切换工具**不**清空（选中是跨工具的持久对象状态）。

use std::collections::HashSet;

use super::shape_tool::ShapeKind;

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

/// 一个已确认的绘制图形对象
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnShape {
    /// 稳定标识（注册表内自增，选中态以此引用）
    pub id: u64,
    /// 所属音轨（仅在当前轨可点选 / 高亮）
    pub track: usize,
    /// 创建该图形的音符历史分组 ID（`None` = 未接入历史，撤销时仅随 reset 清空）
    pub group: Option<u64>,
    /// 矢量几何
    pub source: DrawnShapeSource,
}

/// 图形选中状态
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShapeSelectState {
    /// 全部已登记图形（含已被撤销隐藏的）
    shapes: Vec<DrawnShape>,
    /// 下一个自增 ID
    next_id: u64,
    /// 当前选中的图形 ID
    selected: Option<u64>,
    /// 已被撤销（音符已删除）而隐藏的历史分组 ID
    hidden_groups: HashSet<u64>,
}

impl ShapeSelectState {
    /// 清空全部图形与选中态（文档重建时调用）
    pub fn clear(&mut self) {
        self.shapes.clear();
        self.next_id = 0;
        self.selected = None;
        self.hidden_groups.clear();
    }

    /// 登记一个已确认图形，返回其稳定 ID
    pub fn add(
        &mut self,
        track: usize,
        group: Option<u64>,
        source: DrawnShapeSource,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.shapes.push(DrawnShape {
            id,
            track,
            group,
            source,
        });
        id
    }

    /// 全部图形（含隐藏）
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

    /// 选中指定图形；`None` = 取消选中。指向不存在的 ID 时退化为取消选中。
    pub fn select(&mut self, id: Option<u64>) {
        self.selected = match id {
            Some(id) if self.shapes.iter().any(|s| s.id == id) => Some(id),
            _ => None,
        };
    }

    /// 当前选中的图形对象
    pub fn selected_shape(&self) -> Option<&DrawnShape> {
        let id = self.selected?;
        self.shapes.iter().find(|s| s.id == id)
    }

    /// 指定图形是否因历史撤销而隐藏
    pub fn is_hidden(&self, shape: &DrawnShape) -> bool {
        matches!(shape.group, Some(g) if self.hidden_groups.contains(&g))
    }

    /// 某音轨上当前可见（未被撤销隐藏）的图形，按登记顺序
    pub fn visible_on(&self, track: usize) -> impl DoubleEndedIterator<Item = &DrawnShape> {
        self.shapes
            .iter()
            .filter(move |s| s.track == track && !self.is_hidden(s))
    }

    /// 隐藏某历史分组下的全部图形（撤销音符创建时调用）；并取消选中的隐藏图形
    pub fn hide_group(&mut self, group: u64) {
        self.hidden_groups.insert(group);
        if let Some(id) = self.selected
            && let Some(shape) = self.shapes.iter().find(|s| s.id == id)
            && self.is_hidden(shape)
        {
            self.selected = None;
        }
    }

    /// 恢复某历史分组下的全部图形（重做音符创建时调用）
    pub fn show_group(&mut self, group: u64) {
        self.hidden_groups.remove(&group);
    }

    /// 删除指定图形（含选中态收敛）
    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.shapes.len();
        self.shapes.retain(|s| s.id != id);
        if self.selected == Some(id) {
            self.selected = None;
        }
        self.shapes.len() != before
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

    #[test]
    fn test_add_assigns_stable_ids() {
        let mut st = ShapeSelectState::default();
        let a = st.add(1, Some(7), rect_source());
        let b = st.add(1, None, rect_source());
        assert_ne!(a, b, "不同图形必须得到不同 ID");
        assert_eq!(st.len(), 2);
        assert_eq!(st.shapes()[0].id, a);
    }

    #[test]
    fn test_select_validates_existence() {
        let mut st = ShapeSelectState::default();
        let a = st.add(1, None, rect_source());
        st.select(Some(a));
        assert_eq!(st.selected(), Some(a));
        // 不存在的 ID 退化为取消选中，而非留下悬空引用
        st.select(Some(9999));
        assert_eq!(st.selected(), None);
        st.select(Some(a));
        st.select(None);
        assert_eq!(st.selected(), None);
    }

    #[test]
    fn test_visible_on_filters_track_and_hidden() {
        let mut st = ShapeSelectState::default();
        st.add(1, Some(10), rect_source());
        st.add(2, Some(10), rect_source());
        st.add(1, Some(20), rect_source());
        assert_eq!(st.visible_on(1).count(), 2, "轨道 1 有两条可见图形");
        st.hide_group(10);
        assert_eq!(st.visible_on(1).count(), 1, "隐藏组 10 后轨道 1 剩一条");
        assert_eq!(st.visible_on(2).count(), 0);
        st.show_group(10);
        assert_eq!(st.visible_on(1).count(), 2, "重做恢复后重新可见");
    }

    #[test]
    fn test_hide_group_clears_selection_of_hidden_shape() {
        let mut st = ShapeSelectState::default();
        let a = st.add(1, Some(10), rect_source());
        st.select(Some(a));
        st.hide_group(10);
        assert_eq!(st.selected(), None, "被隐藏的图形不应保持选中");
    }

    #[test]
    fn test_remove_clears_selection() {
        let mut st = ShapeSelectState::default();
        let a = st.add(1, None, rect_source());
        st.select(Some(a));
        assert!(st.remove(a));
        assert_eq!(st.selected(), None);
        assert!(st.is_empty());
        assert!(!st.remove(a), "重复删除返回 false");
    }

    #[test]
    fn test_clear_resets_everything() {
        let mut st = ShapeSelectState::default();
        let a = st.add(1, Some(3), rect_source());
        st.select(Some(a));
        st.clear();
        assert!(st.is_empty());
        assert_eq!(st.selected(), None);
        // clear 后 ID 重新从 0 分配
        assert_eq!(st.add(1, None, rect_source()), 0);
    }
}
