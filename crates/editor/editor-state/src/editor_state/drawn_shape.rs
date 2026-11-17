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

    /// 登记一个已确认图形（几何 + 它生成的音符），返回其稳定 ID
    pub fn add(
        &mut self,
        track: usize,
        group: Option<u64>,
        source: DrawnShapeSource,
        notes: Vec<ShapeNote>,
    ) -> u64 {
        self.push_shape(DrawnShape {
            id: self.next_id,
            model_id: None,
            track,
            group,
            source,
            notes,
            hidden_by_creation: false,
            deleted: false,
            ever_deleted: false,
            moves: Vec::new(),
            pending: None,
        })
    }

    /// 登记一个**待确认产物的镜像**（未 √ 的几何：无音符、无历史分组），返回其稳定 ID
    ///
    /// 两阶段交互下，未 √ 的曲线 / 形状 / 画刷在鼠标工具下照旧渲染（否则用户
    /// 「切过去图案就消失」），既然看得见就必须框得中、拖得动——本方法把它们纳入
    /// 选中域。音符仍只在 √ 时生成：镜像的 `notes` 恒空，移动 / 删除由调用方
    /// 写回 owning tool 的待确认几何（见 `Editor::translate_pending`）。
    pub fn add_pending(
        &mut self,
        track: usize,
        pending: PendingShapeRef,
        source: DrawnShapeSource,
    ) -> u64 {
        self.push_shape(DrawnShape {
            id: self.next_id,
            model_id: None,
            track,
            group: None,
            source,
            notes: Vec::new(),
            hidden_by_creation: false,
            deleted: false,
            ever_deleted: false,
            moves: Vec::new(),
            pending: Some(pending),
        })
    }

    /// 入表 + 自增 ID（`add` / `add_pending` 共用）
    fn push_shape(&mut self, shape: DrawnShape) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.shapes.push(shape);
        id
    }

    // ── 待确认镜像 ─────────────────────────────────────────

    /// 指定图形的待确认来源（已确认图形返回 `None`）
    pub fn pending_of(&self, id: u64) -> Option<PendingShapeRef> {
        self.shapes
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.pending)
    }

    /// 某待确认来源对应的镜像 ID（无镜像返回 `None`）
    pub fn pending_mirror(&self, r: PendingShapeRef) -> Option<u64> {
        self.shapes
            .iter()
            .find(|s| s.pending == Some(r))
            .map(|s| s.id)
    }

    /// 覆写某图形的矢量几何（镜像几何跟随 owning tool 的待确认几何更新）。
    ///
    /// 唯一权威始终是 owning tool 的容器：镜像只是派生视图，故每次写回 / 平移后
    /// 都用「由 owning tool 重新导出」的几何覆写，杜绝两处几何各自演化而漂移。
    pub fn set_source(&mut self, id: u64, source: DrawnShapeSource) -> bool {
        let Some(s) = self.shapes.iter_mut().find(|s| s.id == id) else {
            return false;
        };
        s.source = source;
        true
    }

    /// 绑定/解绑 `ShapeStore` 模型 id
    pub fn set_model_id(&mut self, id: u64, model_id: Option<u64>) -> bool {
        let Some(shape) = self.shapes.iter_mut().find(|shape| shape.id == id) else {
            return false;
        };
        shape.model_id = model_id;
        true
    }

    /// 该图形绑定的模型 id
    pub fn model_id_of(&self, id: u64) -> Option<u64> {
        self.shapes
            .iter()
            .find(|shape| shape.id == id)
            .and_then(|shape| shape.model_id)
    }

    /// 记录 019–022 的还原快照
    pub fn set_restore(&mut self, id: u64, shapes: Vec<DrawnShape>) {
        self.restore.insert(id, shapes);
    }

    /// 取出还原快照（一次性）
    pub fn take_restore(&mut self, id: u64) -> Option<Vec<DrawnShape>> {
        self.restore.remove(&id)
    }

    /// 删除**全部**待确认镜像（离开鼠标工具时调用；已确认图形不受影响）
    pub fn remove_pending_all(&mut self) {
        self.shapes.retain(|s| s.pending.is_none());
        self.collapse_selection();
    }

    /// 待确认容器里删掉一件后，把同容器中**下标更大**的镜像前移一位
    ///
    /// 只有同容器（同一 `PendingShapeRef` 变体）且下标在被删元素之后的镜像会受
    /// 下标位移影响；几何本身不变，故无需重建镜像（ID / 选中集都保持稳定）。
    pub fn shift_pending_indices(&mut self, removed: PendingShapeRef) {
        for s in &mut self.shapes {
            let Some(p) = s.pending.as_mut() else {
                continue;
            };
            let (cur, gone) = match (p, removed) {
                (PendingShapeRef::Shape(i), PendingShapeRef::Shape(k)) => (i, k),
                (PendingShapeRef::CurvePath(i), PendingShapeRef::CurvePath(k)) => (i, k),
                (PendingShapeRef::BrushStroke(i), PendingShapeRef::BrushStroke(k)) => (i, k),
                (PendingShapeRef::Composed(i), PendingShapeRef::Composed(k)) => (i, k),
                _ => continue,
            };
            if *cur > gone {
                *cur -= 1;
            }
        }
    }

    /// 图形是否属于给定音轨的**可交互域**（可见 / 可选中 / 可拖动）
    ///
    /// - 已确认图形：严格归其登记音轨；
    /// - 待确认镜像：几何尚未绑定音轨（owning 预览在任何非 Conductor 轨都渲染），
    ///   故在任何非 Conductor 轨都可交互——否则用户切轨后又会看到「看得见框不中」。
    fn in_track(s: &DrawnShape, track: usize) -> bool {
        if s.pending.is_some() {
            track != 0
        } else {
            s.track == track
        }
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

    /// 全部待确认镜像（几何 + 来源），按登记顺序
    pub fn pending_shapes(&self) -> impl Iterator<Item = (&DrawnShape, PendingShapeRef)> {
        self.shapes.iter().filter_map(|s| s.pending.map(|p| (s, p)))
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
                let _ = s.source.translate(-mv.dtick, -mv.dkey);
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
                let _ = s.source.translate(mv.dtick, mv.dkey);
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
        let (dtick, dkey) = s.source.translate(dtick, dkey);
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

    /// 开始拖拽**选中集合**（无选中时不进入拖拽态）
    ///
    /// 与拉框**互斥**：若此前处于拉框态则一并收敛（同一时刻只会有一个手势在跑，
    /// 避免 `moved`/`released` 的早退分支误把拉框事件当成拖拽来消费）。
    pub fn begin_drag(&mut self, start_tick: f32, start_key: f32) {
        if self.selected.is_empty() {
            return;
        }
        self.marquee = None;
        let bounds = self
            .selected_shapes()
            .filter_map(|s| s.source.bounds())
            .fold(None::<(f32, f32, f32)>, |acc, b| {
                Some(match acc {
                    None => (b.0, b.2, b.3),
                    Some((min_tick, min_key, max_key)) => {
                        (min_tick.min(b.0), min_key.min(b.2), max_key.max(b.3))
                    }
                })
            });
        self.drag = Some(ShapeDrag {
            start_tick,
            start_key,
            delta_tick: 0.0,
            delta_key: 0.0,
            bounds,
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
        let raw_dtick = (raw_dtick / snap).round() * snap;
        let raw_dkey = (key - d.start_key).round();
        // 按拖拽起点的几何边界整体夹取：越过 tick 0 时停住，不压扁图案（DEBT-20）
        let (delta_tick, delta_key) = match d.bounds {
            Some((min_tick, min_key, max_key)) => {
                clamp_translate_delta(raw_dtick, raw_dkey, min_tick, min_key, max_key)
            }
            None => (raw_dtick, raw_dkey),
        };
        d.delta_tick = delta_tick;
        d.delta_key = delta_key;
    }

    /// 当前拖拽状态
    pub fn drag(&self) -> Option<ShapeDrag> {
        self.drag
    }

    /// 是否正在拖拽图形
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// 结束拖拽并返回 `(dtick, dkey)`；无实际位移时返回 `None`
    pub fn end_drag(&mut self) -> Option<(f32, f32)> {
        let d = self.drag.take()?;
        if d.delta_tick == 0.0 && d.delta_key == 0.0 {
            return None;
        }
        Some((d.delta_tick, d.delta_key))
    }

    // ── 空白拉框（框选） ───────────────────────────────────

    /// 从空白处起框（框选）；`additive` = 起框时按住修饰键 → 松手并入既有选中集
    ///
    /// 与拖拽**互斥**：若此前处于拖拽态则一并收敛（理由同 `begin_drag`）。
    pub fn begin_marquee(&mut self, tick: f32, key: f32, additive: bool) {
        self.drag = None;
        self.marquee = Some(ShapeMarquee {
            start_tick: tick,
            start_key: key,
            cur_tick: tick,
            cur_key: key,
            additive,
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

    /// 删除指定图形记录（含选中集收敛）
    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.shapes.len();
        self.shapes.retain(|s| s.id != id);
        self.selected.retain(|&x| x != id);
        self.shapes.len() != before
    }

    /// 收敛选中集：剔除已不可见 / 已不存在的图形
    fn collapse_selection(&mut self) {
        let alive: Vec<u64> = self
            .shapes
            .iter()
            .filter(|s| s.is_visible())
            .map(|s| s.id)
            .collect();
        self.selected.retain(|id| alive.contains(id));
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
mod tests;
