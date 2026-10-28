//! 形状抽屉的纯状态文档（DRAW-033B1）
//!
//! 坐标统一使用单位框 `0..=1`；画布只负责屏幕映射，不把 zoom/pan 写进
//! `ShapeDrawing`。本模块不依赖 iced widget，便于对六工具、选择与撤销做单测。

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use lumino_core::shape::{AreaMap, AreaSpot, ShapeDrawing, Stroke, StrokeRole};

mod areas;
pub mod draft;
mod transform;
pub use draft::Draft;

/// 抽屉工具
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DrawerTool {
    /// 自由笔
    #[default]
    Pen,
    /// 直线
    Line,
    /// 曲线
    Curve,
    /// 三点弧
    Arc,
    /// 矩形
    Rect,
    /// 圆
    Circle,
    /// 选择
    Select,
}

impl DrawerTool {
    /// 工具栏顺序
    pub const ALL: [Self; 7] = [
        Self::Pen,
        Self::Line,
        Self::Curve,
        Self::Arc,
        Self::Rect,
        Self::Circle,
        Self::Select,
    ];

    /// 画布工具栏短标签（避免 CJK canvas 字体回退问题）
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pen => "Pen",
            Self::Line => "Line",
            Self::Curve => "Curve",
            Self::Arc => "Arc",
            Self::Rect => "Rect",
            Self::Circle => "Circle",
            Self::Select => "Sel",
        }
    }
}

/// 画布视图（屏幕缩放/平移，不写回图形）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawerView {
    /// 缩放倍率
    pub zoom: f32,
    /// 平移（单位框坐标）
    pub pan: (f32, f32),
}

impl Default for DrawerView {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: (0.0, 0.0),
        }
    }
}

impl DrawerView {
    /// 缩放并夹到安全范围
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(0.25, 8.0);
    }

    /// 以视图中心为锚点滚轮缩放
    pub fn zoom_by(&mut self, factor: f32) {
        if factor.is_finite() && factor > 0.0 {
            self.set_zoom(self.zoom * factor);
        }
    }
}

#[derive(Debug, Clone)]
struct Snapshot {
    strokes: Vec<Stroke>,
    roles: Vec<StrokeRole>,
    areas: Vec<AreaSpot>,
    selection: BTreeSet<usize>,
}

/// 抽屉文档
#[derive(Debug, Clone)]
pub struct DrawerDoc {
    /// 当前工具
    pub tool: DrawerTool,
    /// 单位框内的 strokes
    pub strokes: Vec<Stroke>,
    /// 选中的 stroke 下标
    pub selection: BTreeSet<usize>,
    /// 与 strokes 等长的角色
    pub roles: Vec<StrokeRole>,
    /// 归一化区域点
    pub areas: Vec<AreaSpot>,
    /// 是否处于区域着色模式
    pub area_mode: bool,
    /// 当前区域色（0..=15）
    pub area_colour: u8,
    /// 画布视图
    pub view: DrawerView,
    /// 是否有未保存修改
    pub dirty: bool,
    /// 文档修订号；区域缓存按它失效
    revision: u64,
    /// 区域网格缓存（Rc 避免每帧复制 1024² 网格）
    area_cache: RefCell<Option<(u64, Rc<AreaMap>)>>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Default for DrawerDoc {
    fn default() -> Self {
        Self {
            tool: DrawerTool::default(),
            strokes: Vec::new(),
            selection: BTreeSet::new(),
            roles: Vec::new(),
            areas: Vec::new(),
            area_mode: false,
            area_colour: 1,
            view: DrawerView::default(),
            dirty: false,
            revision: 0,
            area_cache: RefCell::new(None),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }
}

impl DrawerDoc {
    /// 切换工具；选择工具以外清空选择
    pub fn set_tool(&mut self, tool: DrawerTool) {
        self.tool = tool;
        if tool != DrawerTool::Select {
            self.selection.clear();
        }
    }

    /// 记录一次可撤销变更
    fn checkpoint(&mut self) {
        self.undo.push(Snapshot {
            strokes: self.strokes.clone(),
            roles: self.roles.clone(),
            areas: self.areas.clone(),
            selection: self.selection.clone(),
        });
        if self.undo.len() > 128 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1);
    }

    /// 追加一条 stroke（一次撤销单元）
    pub fn add_stroke(&mut self, stroke: Stroke) {
        self.checkpoint();
        self.strokes.push(stroke);
        self.roles.push(StrokeRole::default());
        self.selection.clear();
    }

    /// 删除选中 strokes（一次撤销单元）
    pub fn delete_selected(&mut self) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        self.checkpoint();
        let selected: Vec<usize> = self.selection.iter().copied().collect();
        for index in selected.into_iter().rev() {
            if index < self.strokes.len() {
                self.strokes.remove(index);
            }
            if index < self.roles.len() {
                self.roles.remove(index);
            }
        }
        self.selection.clear();
        true
    }

    /// 是否可撤销
    pub fn undo_empty(&self) -> bool {
        self.undo.is_empty()
    }

    /// 是否可重做
    pub fn redo_empty(&self) -> bool {
        self.redo.is_empty()
    }

    /// 撤销；返回是否发生变化
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(Snapshot {
            strokes: self.strokes.clone(),
            roles: self.roles.clone(),
            areas: self.areas.clone(),
            selection: self.selection.clone(),
        });
        self.strokes = previous.strokes;
        self.roles = previous.roles;
        self.areas = previous.areas;
        self.selection = previous.selection;
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// 重做；返回是否发生变化
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(Snapshot {
            strokes: self.strokes.clone(),
            roles: self.roles.clone(),
            areas: self.areas.clone(),
            selection: self.selection.clone(),
        });
        self.strokes = next.strokes;
        self.roles = next.roles;
        self.areas = next.areas;
        self.selection = next.selection;
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// 点选最近的 stroke；命中返回 true
    pub fn select_at(&mut self, point: (f32, f32), tolerance: f32) -> bool {
        let mut best: Option<(usize, f32)> = None;
        for (index, stroke) in self.strokes.iter().enumerate() {
            let distance = stroke_distance(stroke, point);
            if distance <= tolerance && best.is_none_or(|(_, current)| distance < current) {
                best = Some((index, distance));
            }
        }
        self.selection.clear();
        if let Some((index, _)) = best {
            self.selection.insert(index);
            true
        } else {
            false
        }
    }

    /// 选中高亮判断
    pub fn is_selected(&self, index: usize) -> bool {
        self.selection.contains(&index)
    }

    /// 把一次拖拽/点击草稿提交为 stroke
    pub fn commit_draft(&mut self, stroke: Option<Stroke>) -> bool {
        let Some(stroke) = stroke else {
            return false;
        };
        self.add_stroke(stroke);
        true
    }

    /// 清空全部 strokes（一次撤销单元；B1 调试/重置用）
    pub fn clear(&mut self) -> bool {
        if self.strokes.is_empty() {
            return false;
        }
        self.checkpoint();
        self.strokes.clear();
        self.roles.clear();
        self.areas.clear();
        self.selection.clear();
        true
    }

    /// 从库图形载入到画板；重置选择与撤销栈。
    pub fn load_drawing(&mut self, drawing: &ShapeDrawing) {
        self.strokes = drawing.strokes.clone();
        self.roles = drawing.roles.clone();
        self.areas = drawing.areas.clone();
        if self.roles.len() != self.strokes.len() {
            self.roles = vec![StrokeRole::default(); self.strokes.len()];
        }
        self.selection.clear();
        self.undo.clear();
        self.redo.clear();
        self.dirty = false;
        self.area_mode = false;
        self.revision = self.revision.wrapping_add(1);
    }

    /// 导出为库图形；roles 不足时补默认值。
    pub fn to_drawing(&self, id: String, name: String) -> ShapeDrawing {
        let roles = if self.roles.len() == self.strokes.len() {
            self.roles.clone()
        } else {
            vec![StrokeRole::default(); self.strokes.len()]
        };
        ShapeDrawing {
            id,
            name,
            strokes: self.strokes.clone(),
            areas: self.areas.clone(),
            roles,
        }
    }
}

/// 交互草稿（与 DrawerDoc 分开，画布重建时仍可恢复）
#[derive(Debug, Clone, Default)]
pub struct DrawerInteraction {
    /// 当前草稿
    pub draft: Option<Draft>,
    /// 中键平移起点（屏幕局部坐标）
    pub pan_start: Option<(f32, f32)>,
    /// Shift 是否按下（Rect/Circle 约束正方形/正圆）
    pub shift: bool,
}

fn distance(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn stroke_distance(stroke: &Stroke, point: (f32, f32)) -> f32 {
    let mut best = f32::INFINITY;
    let anchors = &stroke.anchors;
    if anchors.len() == 1 {
        return distance(anchors[0].pos, point);
    }
    for window in anchors.windows(2) {
        best = best.min(segment_distance(window[0].pos, window[1].pos, point));
    }
    best
}

fn segment_distance(a: (f32, f32), b: (f32, f32), p: (f32, f32)) -> f32 {
    let ab = (b.0 - a.0, b.1 - a.1);
    let len2 = ab.0 * ab.0 + ab.1 * ab.1;
    if len2 <= 1e-9 {
        return distance(a, p);
    }
    let t = (((p.0 - a.0) * ab.0 + (p.1 - a.1) * ab.1) / len2).clamp(0.0, 1.0);
    distance((a.0 + ab.0 * t, a.1 + ab.1 * t), p)
}

#[cfg(test)]
mod tests;
