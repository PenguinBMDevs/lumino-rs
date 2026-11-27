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

use std::collections::{HashMap, HashSet};

use super::shape_tool::ShapeKind;

/// 某个图形 √ 确认时生成的**单个音符**（逻辑值，用于整体移动 / 删除）
///
/// 记录定位所需的最小字段：`(track, tick, key, length, velocity)`——与文档里
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
    /// 力度（1..=127；形状样式 velocity 为初值，算子可改）
    pub velocity: u8,
    /// MIDI 通道（0..=15；Cycle 算子分配）
    pub channel: u8,
}

impl ShapeNote {
    /// 平移出新音符（tick / key 位移，长度不变）
    pub fn translated(&self, dtick: f32, dkey: f32) -> Self {
        Self {
            track: self.track,
            tick: self.tick + dtick,
            key: (self.key as f32 + dkey).round().clamp(0.0, 255.0) as u16,
            length: self.length,
            velocity: self.velocity,
            channel: self.channel,
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
        /// 外接框逻辑坐标 (tick_lo, key_lo, tick_hi, key_hi)（已规范化 lo <= hi）
        rect: (f32, f32, f32, f32),
        /// 绘制时是否按住 Shift（屏幕空间正图形约束）
        shift_constrained: bool,
        /// 是否填充内部（仅作信息保留，供后续编辑回放）
        filled: bool,
        /// 三角形顶点朝向（顶点是否在 key 大的一侧；语义见 `ShapeSpec::apex_high`）
        ///
        /// 必须随几何一起登记：外接框已规范化、朝向无法从中反推，
        /// 丢了它鼠标工具高亮 / 命中就会把三角形画成镜像。
        apex_high: bool,
    },
    /// 折线（曲线工具展平路径 / 画刷笔画），逻辑坐标 (tick, key)
    Polyline {
        /// 折线点列（至少 1 个点）
        points: Vec<(f32, f32)>,
    },
    /// 多条折线（Bake 前组合预览：Live / Joined 的多段描边）
    PolylineSet {
        /// 每条折线的点列；空条目会被渲染/命中跳过
        polylines: Vec<Vec<(f32, f32)>>,
    },
}

/// 按几何边界夹取整体平移量：tick 不低于 0，key 保持在 0..=255。
///
/// 返回**实际可应用**的 `(dtick, dkey)`。必须按「整体」夹取——逐坐标 `max(0)`
/// 会在越过 tick 0 时把矩形/折线压扁（DEBT-20：拖左再拖回，选框比图案短）。
pub fn clamp_translate_delta(
    dtick: f32,
    dkey: f32,
    min_tick: f32,
    min_key: f32,
    max_key: f32,
) -> (f32, f32) {
    (
        if min_tick.is_finite() {
            dtick.max(-min_tick)
        } else {
            dtick
        },
        if min_key.is_finite() && max_key.is_finite() {
            dkey.clamp(-min_key, 255.0 - max_key)
        } else {
            dkey
        },
    )
}

impl DrawnShapeSource {
    /// 整体平移几何（tick 自由、key 取整后钳到 0..=255）
    ///
    /// 返回**实际应用**的 `(dtick, dkey)`：按整体边界夹取，避免越过 tick 0 时
    /// 逐坐标 `max(0)` 把矩形/折线压扁（DEBT-20）。
    pub fn translate(&mut self, dtick: f32, dkey: f32) -> (f32, f32) {
        let (dtick, dkey) = match self.bounds() {
            Some((min_tick, _, min_key, max_key)) => {
                clamp_translate_delta(dtick, dkey, min_tick, min_key, max_key)
            }
            None => (dtick, dkey),
        };
        match self {
            Self::Shape { rect, .. } => {
                rect.0 += dtick;
                rect.2 += dtick;
                rect.1 += dkey;
                rect.3 += dkey;
            }
            Self::Polyline { points } => {
                for p in points.iter_mut() {
                    p.0 += dtick;
                    p.1 += dkey;
                }
            }
            Self::PolylineSet { polylines } => {
                for points in polylines {
                    for p in points.iter_mut() {
                        p.0 += dtick;
                        p.1 += dkey;
                    }
                }
            }
        }
        (dtick, dkey)
    }

    /// 外接框（逻辑坐标 (min_tick, max_tick, min_key, max_key)）；空折线返回 `None`
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        match self {
            Self::Shape { rect, .. } => {
                let (x0, y0, x1, y1) = *rect;
                Some((x0.min(x1), x0.max(x1), y0.min(y1), y0.max(y1)))
            }
            Self::Polyline { points } => bounds_of(points),
            Self::PolylineSet { polylines } => {
                let mut acc: Option<(f32, f32, f32, f32)> = None;
                for points in polylines {
                    let Some(b) = bounds_of(points) else {
                        continue;
                    };
                    acc = Some(match acc {
                        None => b,
                        Some(a) => (a.0.min(b.0), a.1.max(b.1), a.2.min(b.2), a.3.max(b.3)),
                    });
                }
                acc
            }
        }
    }
}

/// 单条折线的外接框
fn bounds_of(points: &[(f32, f32)]) -> Option<(f32, f32, f32, f32)> {
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

/// 待确认产物的来源（未 √ 的绘制几何）
///
/// 曲线 / 形状 / 画刷的**两阶段交互**：拖动只产生「待确认几何」，按 √ 才生成音符。
/// 鼠标工具（框选）下这些几何照旧渲染（`Editor::pending_preview_visible`），
/// 因此它们必须同时进入图形选中域——否则用户「看得见却框不中、拖不动」。
///
/// 携带来源的工具与容器下标，作用有二：
/// - **移动 / 删除写回**：待确认几何仍以 owning tool 的容器为唯一权威，
///   选中域里的镜像只是它的派生视图（见 `ShapeSelectState::add_pending`）；
/// - **几何同步**：镜像不自己演化——每次写回 / 平移都由 owning 侧重新导出几何覆写
///   （见 `ShapeSelectState::set_source`），保证预览与选框读到的永远是同一份几何。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingShapeRef {
    /// 形状工具第 `i` 个待确认图形（`shape_tool.shapes[i]`）
    Shape(usize),
    /// 曲线工具第 `i` 条待确认路径（`line_tool.paths[i]`）
    CurvePath(usize),
    /// 画刷工具第 `i` 条待确认笔画（`brush_tool.strokes[i]`）
    BrushStroke(usize),
    /// Bake 前组合会话第 `i` 个组合结果（`EditorState::pending_composed[i]`）
    Composed(usize),
}

/// 一个已确认的绘制图形对象
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnShape {
    /// 稳定标识（注册表内自增，选中态以此引用）
    pub id: u64,
    /// 对应的 `ShapeStore` 模型 id（019–022 的矢量编辑入口）
    pub model_id: Option<u64>,
    /// 所属音轨（仅在当前轨可点选 / 高亮 / 编辑）
    ///
    /// 待确认镜像（`pending.is_some()`）例外：几何尚未绑定音轨，随任何非
    /// Conductor 轨可见可选中（见 [`ShapeSelectState::in_track`]）。
    pub track: usize,
    /// 创建该图形的音符历史分组 ID（`None` = 未接入历史）
    pub group: Option<u64>,
    /// 矢量几何
    pub source: DrawnShapeSource,
    /// 该图形 √ 确认时生成的音符（整体移动 / 删除的作用对象）
    ///
    /// 待确认镜像恒为空：未 √ 就没有音符，移动 / 删除只作用于几何。
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
    /// 待确认几何的来源（`Some` = 未 √ 的镜像，见 [`PendingShapeRef`]）
    pub pending: Option<PendingShapeRef>,
}

impl DrawnShape {
    /// 当前是否可见（未被撤销创建、未被删除）
    pub fn is_visible(&self) -> bool {
        !self.hidden_by_creation && !self.deleted
    }
}

/// 图形拖拽移动状态（鼠标工具在**选中集合**上拖动）
///
/// **不记录具体图形 ID**：拖动作用于整个选中集合（单选时集合里恰好一个），
/// 因此「点选后拖动」与「框选多个后整组拖动」走的是同一条链路，无需分支。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeDrag {
    /// 按下时的逻辑坐标 (tick, key)
    pub start_tick: f32,
    /// 按下时的逻辑 key
    pub start_key: f32,
    /// 当前 tick 偏移
    pub delta_tick: f32,
    /// 当前 key 偏移
    pub delta_key: f32,
    /// 拖拽起点的几何边界 `(min_tick, min_key, max_key)`；用于整体夹取偏移
    pub bounds: Option<(f32, f32, f32)>,
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
    /// 起框时是否按住修饰键（Shift）——为真时松手「并入」既有选中集，否则「替换」
    pub additive: bool,
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

/// 图形选中状态（**多选**）
///
/// 选中集是**有序去重**的图形 ID 列表：点选 / Shift 点选 / 框选都往里加，
/// 移动 / 删除 / 高亮 / 选框一律以「集合」为单位。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShapeSelectState {
    /// 全部已登记图形（含被隐藏 / 已删除的）
    shapes: Vec<DrawnShape>,
    /// 下一个自增 ID
    next_id: u64,
    /// 选中集合（按加入顺序，去重）
    selected: Vec<u64>,
    /// 拖拽移动状态（作用于整个选中集合）
    drag: Option<ShapeDrag>,
    /// 空白处拉框（框选）状态
    marquee: Option<ShapeMarquee>,
    /// 019–022 的“还原原图形”快照（仅会话内；不参与序列化）
    restore: HashMap<u64, Vec<DrawnShape>>,
}

mod select_state;

#[cfg(test)]
mod tests;
