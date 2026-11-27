//! 图形选中工具（音符画工具栏「鼠标工具」）交互 —— 点选 / 拖动移动 / 删除
//!
//! 与「登记」两面一体：
//! - **登记**：曲线 / 形状 / 画刷工具 √ 确认生成音符后，把各自的矢量几何 + 它生成的
//!   音符登记进 `EditorState::shape_select`（见 `record_*` 方法），并绑定本次音符
//!   创建历史的分组 ID，供撤销/重做同步（见 `Editor::undo/redo`）。
//! - **点选**：鼠标工具（`Tool::ShapeSelect`）左键按下 —— 命中当前音轨**最上层**
//!   的可见图形则把它设为唯一选中；按住 **Shift** 则**切换**该图形的选中态（加入 / 移出）。
//!   点空白取消选中。高亮描边 + 选框由 `grid::drawn_shape_box` 渲染。
//! - **框选**：鼠标工具在空白处按下并拖动 → 叠加层实时画出拉框，松手后把框内**全部**
//!   可见图形并入选中集（Shift 起框 = 与既有选中集求并集，否则替换）。拉框过小视为
//!   一次普通点击，不改变选中态（保持「点空白取消选中」语义）。
//! - **选中选框**：选中集的**并集**外接框（含四角手柄）——见 [`Editor::selection_box`]。
//!   渲染与命中同源，是「看得见 ↔ 抓得住」的同一口径。
//! - **拖动移动**：按下并拖动 → 实时预览偏移（叠加层）→ 松手后把选中集内全部图形生成的
//!   音符整体按 `MoveOp`（删旧 + 加新）平移，并同步平移几何；**整组共用一个历史分组**，
//!   一次撤销/重做整组回放。拖动入口**统一**为「命中图形 **或** 落在选中集选框内部」：
//!   点选与框选两种来源共用同一条移动链路。
//! - **删除**：Delete 键或右键菜单「删除」→ 一次快照历史 + 按值删除选中集内全部图形的
//!   音符 + 逐个标记已删除；撤销该删除可恢复。
//!
//! 进入鼠标工具时，其它绘制工具的**待确认内容原样保留**（`EditorState::set_tool` 只收敛
//! 未完成的交互手势、不丢弃产物），且其几何预览在选择工具下照样渲染
//! （`Editor::pending_preview_visible`）——因此「画完切过去」图案不会消失，
//! 也不会被擅自固化成音符（音符只在用户显式 √ 时生成）。
//!
//! 命中判定（逻辑坐标 → 屏幕像素空间，容差统一为像素）：
//! - 形状工具图形：直接复用 `point_in_shape`（含 Shift 正图形约束与圆形内部判定）；
//! - 折线（曲线展平路径 / 画刷笔画）：点到各段的屏幕距离 ≤ [`HIT_TOLERANCE_PX`]。
//!
//! 图形是**叠加对象**（音符才是真正的文档内容）：几何与音符列表登记在注册表，
//! 移动/删除改的是 document（走既有历史/渲染管线）。

use std::collections::{HashMap, HashSet};

use lumino_editor_state::{
    DrawnShapeSource, PendingShapeRef, ShapeKind, ShapeMarquee, ShapeNote,
    shape_tool::{ShapeSpec, effective_rect, point_in_shape},
};
use lumino_message::Tool;
use lumino_midi_loader::NoteEvent;
use lumino_midi_model::TickIndexedEvents;
use lumino_note_core::history::{HistoryEntry, MoveOp, OpKind};
use lumino_ui_core::geom::{Point, Rectangle, Size};

use crate::interaction::line_tool::geom::flatten_path;
use crate::interaction::shape_bridge::polylines_of_model;
use crate::{Editor, Note};

mod hit;

/// 空白拉框的最小屏幕边长（像素）：低于此值视为「一次点击」而非框选
const MARQUEE_MIN_PX: f32 = 4.0;

/// 选中集「选框」相对其几何外接框的四周外扩（像素）
///
/// 让选框略大于图形本身：细线图形（折线）不至于退化成零宽/零高的框，
/// 也让「框内拖动」有一圈可抓取的余量。渲染与命中**共用**本常量，
/// 保证「看到的框」就是「抓得住的框」。
pub(crate) const SHAPE_BOX_PADDING_PX: f32 = 3.0;

/// 单条画刷笔画的登记三元组：(落笔基准轨, 折线点列, 该笔画生成的音符)
#[allow(dead_code)] // 供“显式保留形状”模式/测试复用；Bake 默认不再登记
type StrokeRecord = (usize, Vec<(f32, f32)>, Vec<ShapeNote>);

/// 逻辑 AABB 相交（含接触）；参数布局统一为 `(min_tick, max_tick, min_key, max_key)`
fn aabb_overlap(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    a.0 <= b.1 && b.0 <= a.1 && a.2 <= b.3 && b.2 <= a.3
}

/// 按下标删除；越界返回 `false`（不 panic——下标来自持久化的待确认引用，
/// 任何一处失配都不该把编辑器打崩）
fn remove_at<T>(list: &mut Vec<T>, index: usize) -> bool {
    if index >= list.len() {
        return false;
    }
    list.remove(index);
    true
}

impl Editor {
    // ── 登记 ─────────────────────────────────────────────

    /// 最近一次音符创建历史的**分组 ID**（`push_note_create` 之后调用）。
    ///
    /// 图形对象以此与历史条目绑定：撤销该条目时同组图形一并隐藏，
    /// 重做时恢复。非 Create 条目（或栈顶非 Create）返回 `None`。
    pub(crate) fn last_create_group(&self) -> Option<u64> {
        match self.editor_state.data.history.undo_back() {
            Some(HistoryEntry::Create(entry)) => entry.group_id,
            _ => None,
        }
    }

    /// DRAW-041B：Bake 后保留**只读 ShapeModel**（不登记可拖拽 DrawnShape）
    ///
    /// 模型带本次写出的音符指纹，供 032B/033 的 ShapeStore 列表入口使用；普通 Bake
    /// 仍然不产生可交互图形，保持 DRAW-041 的“确认后只留音符”语义。
    pub(crate) fn record_baked_shape_models(
        &mut self,
        models: Vec<(lumino_core::shape::ShapeModel, Vec<ShapeNote>)>,
    ) {
        let group = self.last_create_group().unwrap_or(0);
        for (model, notes) in models {
            let id = self.editor_state.shape_store.insert(model, group);
            self.editor_state.shape_store.assign_notes(id, notes);
        }
    }

    // ── 待确认产物镜像（未 √ 的几何进入图形选中域） ──────────
    ///
    /// `per_stroke_notes[i]` 与 `brush_tool.strokes[i]` 对应；图形以**落笔基准轨**
    /// （`BrushStroke::base_track`）登记，保证「在哪个轨画的就在哪个轨选中」。
    ///
    /// 当前 Bake 不再登记，仅保留测试入口验证记录语义；生产路径不应调用。
    #[cfg(test)]
    pub(crate) fn record_brush_strokes(&mut self, per_stroke_notes: Vec<Vec<ShapeNote>>) {
        let fallback = self.editor_state.data.current_track;
        let data: Vec<StrokeRecord> = self
            .editor_state
            .brush_tool
            .strokes
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.points.is_empty())
            .map(|(i, s)| {
                (
                    if s.base_track == 0 {
                        fallback
                    } else {
                        s.base_track
                    },
                    s.points.clone(),
                    per_stroke_notes.get(i).cloned().unwrap_or_default(),
                )
            })
            .collect();
        for (track, points, notes) in data {
            record_drawn_shape_on(self, track, DrawnShapeSource::Polyline { points }, notes);
        }
    }

    // ── 待确认产物镜像（未 √ 的几何进入图形选中域） ──────────

    /// 重建「待确认产物」镜像（幂等）
    ///
    /// **为什么必须有镜像**：曲线 / 形状 / 画刷是两阶段交互——拖动只产生待确认几何，
    /// 按 √ 才生成音符；而鼠标工具（框选工具）下这些几何**照旧渲染**
    /// （`Editor::pending_preview_visible`，否则用户「切过去图案就消失」）。
    /// 既然看得见，就必须框得中、拖得动：本方法把每件待确认几何镜像成图形选中域
    /// 条目，供框选命中、选中高亮、选框与拖动复用同一条链路。
    ///
    /// **音符仍只在 √ 时生成**：镜像 `notes` 恒空，移动 / 删除写回 owning tool 的
    /// 待确认容器（见 [`Self::translate_pending`] / [`Self::remove_pending`]），
    /// 文档与历史不受影响。
    ///
    /// 调用时机：切换到鼠标工具（进入选中域）、装载新文档（注册表被清空后）。
    /// 非鼠标工具时清空全部镜像：其它工具下由 owning 预览负责渲染，无需选中域条目。
    pub fn rebuild_pending_mirrors(&mut self) {
        // 重建会换掉镜像 ID：先记下当前选中的待确认来源，重建后按来源恢复选中态。
        // （`set_tool` 可能被同工具重复调用——面板条目重复点击等——重复点击不该清空选区。）
        let keep: Vec<PendingShapeRef> = self
            .editor_state
            .shape_select
            .selected_shapes()
            .filter_map(|s| s.pending)
            .collect();
        self.editor_state.shape_select.remove_pending_all();
        if self.editor_state.current_tool() != Tool::ShapeSelect {
            return;
        }
        let track = self.editor_state.data.current_track;
        // Conductor 轨（track 0）：各 owning 预览与工具本身都不可用（不渲染），
        // 镜像自然不建——「看得见」是「框得着」的前提，看不见就不该凭空可选中。
        if track == 0 {
            return;
        }
        let mut items: Vec<(PendingShapeRef, DrawnShapeSource)> = Vec::new();
        for i in 0..self.editor_state.shape_tool.shapes.len() {
            let r = PendingShapeRef::Shape(i);
            if let Some(src) = self.pending_source(r) {
                items.push((r, src));
            }
        }
        for i in 0..self.editor_state.line_tool.paths.len() {
            let r = PendingShapeRef::CurvePath(i);
            if let Some(src) = self.pending_source(r) {
                items.push((r, src));
            }
        }
        for i in 0..self.editor_state.brush_tool.strokes.len() {
            let r = PendingShapeRef::BrushStroke(i);
            if let Some(src) = self.pending_source(r) {
                items.push((r, src));
            }
        }
        for i in 0..self.editor_state.pending_composed.len() {
            let r = PendingShapeRef::Composed(i);
            if let Some(src) = self.pending_source(r) {
                items.push((r, src));
            }
        }
        for (r, src) in items {
            self.editor_state.shape_select.add_pending(track, r, src);
        }
        for r in keep {
            if let Some(id) = self.editor_state.shape_select.pending_mirror(r) {
                self.editor_state.shape_select.add_to_selection(id);
            }
        }
    }

    /// 待确认几何的矢量来源（**镜像几何的唯一权威 = owning tool 的待确认容器**）
    ///
    /// 每次写回 / 平移后都用本方法重新导出并覆写镜像几何，杜绝「镜像与 owning
    /// 各自演化」的漂移。曲线按与 √ 登记同源的 `flatten_path` 展平（同一口径）。
    pub(crate) fn pending_source(&self, r: PendingShapeRef) -> Option<DrawnShapeSource> {
        match r {
            PendingShapeRef::Shape(i) => {
                let s = self.editor_state.shape_tool.shapes.get(i)?;
                Some(DrawnShapeSource::Shape {
                    kind: s.kind,
                    rect: s.rect,
                    shift_constrained: s.shift_constrained,
                    filled: s.filled,
                    apex_high: s.apex_high,
                })
            }
            PendingShapeRef::CurvePath(i) => {
                let path = self.editor_state.line_tool.paths.get(i)?;
                Some(DrawnShapeSource::Polyline {
                    points: flatten_path(path)
                        .into_iter()
                        .map(|(t, k)| (t as f32, k as f32))
                        .collect(),
                })
            }
            PendingShapeRef::BrushStroke(i) => {
                let s = self.editor_state.brush_tool.strokes.get(i)?;
                Some(DrawnShapeSource::Polyline {
                    points: s.points.clone(),
                })
            }
            PendingShapeRef::Composed(i) => {
                let composed = self.editor_state.pending_composed.get(i)?;
                Some(DrawnShapeSource::PolylineSet {
                    polylines: composed
                        .results
                        .iter()
                        .flat_map(|part| {
                            polylines_of_model(&part.model, f32::from(self.editor_state.view.ppq))
                        })
                        .collect(),
                })
            }
        }
    }

    /// 把镜像几何回灌为 owning tool 的**当前**待确认几何（镜像 ID / 选中态不变）
    ///
    /// 待确认几何的唯一权威是 owning tool 的容器，镜像几何只是派生缓存。凡在鼠标工具
    /// 激活时可能改动待确认几何的入口，都必须回灌一次，否则镜像与预览 / 选框分叉，
    /// 重新落回「看得见却框不中 / 框与图案错位」这一类 BUG：
    /// - 镜像自身的拖动写回（[`Self::sync_pending_drag_geometry`]）；
    /// - **撤销 / 重做待确认路径编辑**（`Editor::undo/redo` 的 brush/line `*_path` 分支）——
    ///   该入口与工具无关，鼠标工具下按 Ctrl+Z 同样会回退待确认几何。
    /// - owning 工具自身的交互（拖动锚点/路径）与 √/×：这些入口都只在 owning 工具激活时
    ///   可达，而离开鼠标工具时镜像已被撤掉，无需回灌。
    ///
    /// owning 几何已整体消失（例如撤销回退了整条路径）→ 镜像一并退场，避免留下
    /// 「画不出来却框得中」的幽灵条目。
    pub(crate) fn refresh_pending_mirror_geometry(&mut self) {
        let mirrors: Vec<(u64, PendingShapeRef)> = self
            .editor_state
            .shape_select
            .pending_shapes()
            .map(|(s, r)| (s.id, r))
            .collect();
        for (id, r) in mirrors {
            match self.pending_source(r) {
                Some(src) => {
                    self.editor_state.shape_select.set_source(id, src);
                }
                None => {
                    self.editor_state.shape_select.remove(id);
                }
            }
        }
    }

    /// 屏幕坐标命中的待确认来源（命中已确认图形时回退到当前选中集里的 pending）。
    ///
    /// 与 [`Self::drawn_shape_pending_at_screen`] 的区别：这里返回具体来源，
    /// 供右键菜单判断「当前工具是不是该 pending 的 owner」。
    pub(crate) fn pending_ref_at_screen(&self, pos: Point) -> Option<PendingShapeRef> {
        if let Some(id) = self.drawn_shape_at_screen(pos)
            && let Some(pending) = self.editor_state.shape_select.pending_of(id)
        {
            return Some(pending);
        }
        // 点没命中形状本体（例如落在选框内的空白处）：
        // 只要该 pending 已被选中，右键仍应落到它身上——否则用户必须先框选
        // 或精确点中音符才能拿到长列表（DEBT-14 反馈）。
        let tick = self.pos_to_tick(pos);
        let key = self.pos_to_raw_key(pos);
        if !self.point_in_selection_box(tick, key) {
            return None;
        }
        self.editor_state
            .shape_select
            .selected_shapes()
            .find_map(|shape| shape.pending)
    }

    /// 当前工具是否应暴露待确认右键菜单。
    ///
    /// 选择工具对所有 pending 开放；形状工具只对形状/组合结果开放。
    /// 曲线/画笔的 pending 组合在 ShapeSelect 下完成（组合预览与 ✓× 由形状预览层承载），
    /// 避免在 owner 工具里生成看不见、也无法确认的 composed 结果。
    pub(crate) fn pending_menu_allowed(&self, pending: PendingShapeRef) -> bool {
        match self.editor_state.current_tool() {
            Tool::ShapeSelect => true,
            Tool::Shape => matches!(
                pending,
                PendingShapeRef::Shape(_) | PendingShapeRef::Composed(_)
            ),
            _ => false,
        }
    }

    /// 右键命中的图形是否属于“待确认”菜单目标。
    ///
    /// 返回：
    /// - `Some(true)`：命中 pending 镜像，或当前选中集含 pending（混合选区也按 pending 处理）；
    /// - `Some(false)`：命中已确认图形且选中集全是已确认图形；
    /// - `None`：没有命中图形。
    ///
    /// 这样右键菜单不会把 019–028 暴露给没有 `ShapeStore` 模型 / 音符的 pending 产物。
    pub fn drawn_shape_pending_at_screen(&self, pos: lumino_ui_core::geom::Point) -> Option<bool> {
        let id = self.drawn_shape_at_screen(pos)?;
        let hit_pending = self.editor_state.shape_select.pending_of(id).is_some();
        Some(hit_pending || self.has_pending_shape_selection())
    }

    // ── 选框（选中集外接框） ───────────────────────────────

    /// 选中集合的**选框**（屏幕空间轴对齐矩形，含拖拽实时预览偏移）
    ///
    /// 多选时是**整组的外接框**；返回值已按四周 [`SHAPE_BOX_PADDING_PX`] 外扩，
    /// 是**唯一的选框口径**：
    /// - 渲染（`grid::drawn_shape_box`）用它画框与四角手柄；
    /// - 命中（[`Self::point_in_selection_box`]）用它判定「框内拖动」。
    ///
    /// 渲染与命中同源，杜绝「看到的框抓不住 / 抓得住的框看不见」。
    /// 当前音轨无选中、几何全为空（空折线）时返回 `None`。
    ///
    /// **预览偏移按件区分**：已确认图形走「幽灵」语义（文档未变，几何在原位，
    /// 选框需叠加拖拽偏移才跟手）；待确认镜像的几何已随拖动**即时落位**
    /// （见 [`Self::sync_pending_drag_geometry`]），再叠偏移就成了双倍位移。
    pub fn selection_box(&self) -> Option<Rectangle> {
        let track = self.editor_state.data.current_track;
        let (dtick, dkey) = self.drag_preview_delta();
        // 命中集在**屏幕空间**取并集：逻辑空间与屏幕不是线性同尺度（纵横缩放不同），
        // 先各自换算再合并，才能保证框住每个选中图形（含 padding 外扩）。
        let mut acc: Option<(f32, f32, f32, f32)> = None;
        for s in self.editor_state.shape_select.selected_shapes_on(track) {
            let Some(b) = s.source.bounds() else {
                continue;
            };
            let (dt, dk) = if s.pending.is_some() {
                (0.0, 0.0)
            } else {
                (dtick, dkey)
            };
            let a = self.line_pos_screen_pos((b.0 + dt, b.2 + dk));
            let c = self.line_pos_screen_pos((b.1 + dt, b.3 + dk));
            let r = (a.x.min(c.x), a.y.min(c.y), a.x.max(c.x), a.y.max(c.y));
            acc = Some(match acc {
                None => r,
                Some(p) => (p.0.min(r.0), p.1.min(r.1), p.2.max(r.2), p.3.max(r.3)),
            });
        }
        let (x0, y0, x1, y1) = acc?;
        Some(Rectangle::new(
            Point::new(x0 - SHAPE_BOX_PADDING_PX, y0 - SHAPE_BOX_PADDING_PX),
            Size::new(
                ((x1 - x0) + SHAPE_BOX_PADDING_PX * 2.0).max(1.0),
                ((y1 - y0) + SHAPE_BOX_PADDING_PX * 2.0).max(1.0),
            ),
        ))
    }

    /// 当前拖拽的实时预览偏移 `(dtick, dkey)`；未拖拽时为 `(0, 0)`
    ///
    /// 拖拽作用于**整个选中集合**，故这里是「整组共同偏移」——渲染侧对每个选中图形
    /// 统一套用，无需按图形区分。
    pub fn drag_preview_delta(&self) -> (f32, f32) {
        match self.editor_state.shape_select.drag() {
            Some(d) => (d.delta_tick, d.delta_key),
            None => (0.0, 0.0),
        }
    }

    /// 逻辑坐标点是否落在选中集的选框内（「框内拖动移动整组」的命中判定）
    pub(crate) fn point_in_selection_box(&self, tick: f32, key: f32) -> bool {
        let Some(rect) = self.selection_box() else {
            return false;
        };
        rect.contains(self.line_pos_screen_pos((tick, key)))
    }

    /// 选中一个绘制图形（供右键菜单等外部入口）
    ///
    /// 该图形**已在选中集内时保持多选不变**——使右键菜单的「删除」作用于既有批量选区，
    /// 与音符右键「命中已在选中集合内则不动选区」的语义一致；否则退化为唯一选中它。
    pub fn select_drawn_shape(&mut self, id: u64) {
        if !self.editor_state.shape_select.is_selected(id) {
            self.editor_state.shape_select.select_only(id);
        }
    }

    // ── 点选 / 拖动 ───────────────────────────────────────

    /// 鼠标工具：左键按下 —— 点选并起拖 / 框内起拖 / 空白起框
    ///
    /// `additive` = 按住修饰键（Shift）：命中图形时**切换**其选中态（加/减）；
    /// 空白起框时松手**并入**既有选中集而非替换。
    ///
    /// 拖动目标确定顺序（**统一点选与框选两种来源的移动方式**）：
    ///
    /// ① 命中某图形。`additive` → 切换该图形选中态（切换后仍选中才进入拖拽）；
    /// 否则它**不在**选中集内 → 以它为唯一选中（替换）；否则它**在**选中集内 →
    /// 保持整个选中集不变（于是「点中多选之一再拖」= 整组移动）。
    /// 最终只要它处于选中态就进入拖拽。
    ///
    /// ② 未命中图形，但落在**选中集选框内部** → 拖动整组（无需精确点中细线）。
    ///
    /// ③ 都不满足 → 空白按下：非 `additive` 时取消选中，并起框（是否构成框选由松手时的
    /// 屏幕尺度判定，见 [`Self::finish_shape_marquee`]）。
    pub(crate) fn handle_shape_select_pressed(&mut self, tick: f32, key: f32, additive: bool) {
        // 新一次拖动从零开始：已写回待确认几何的累计偏移必须归零，否则
        // `sync_pending_drag_geometry` 会按上一次的残值算出错误增量。
        self.shape_tools.reset_pending_drag_applied();
        // 手势边界自愈：命中判定与选框都建立在镜像几何之上，动手前先保证它是最新的
        // （任何未预料到的待确认几何改动都不会再退化成「框不中」）。
        self.refresh_pending_mirror_geometry();
        if self.begin_shape_edit(tick, key) {
            return;
        }
        // 020 编辑图形：锚点/旋转热区之外只处理「框内拖动」。
        //   单击不退出编辑态（拖动一次没抓准就丢状态，很难操作），
        //   退出走双击空白处或右键「编辑图形」再点一次。
        if self.is_shape_edit_armed() {
            if self.point_in_selection_box(tick, key) {
                self.editor_state.shape_select.begin_drag(tick, key);
            }
            return;
        }
        match self.hit_test_drawn_shape(tick, key) {
            Some(id) => {
                if additive {
                    self.editor_state.shape_select.toggle_selection(id);
                } else if !self.editor_state.shape_select.is_selected(id) {
                    self.editor_state.shape_select.select_only(id);
                }
                // 已 Bake 图形预先绑定模型，手柄才能在选中后立即显示/命中。
                if self.editor_state.shape_select.is_selected(id) {
                    let _ = self.ensure_model_for_drawn(id);
                    self.editor_state.shape_select.begin_drag(tick, key);
                }
            }
            None => {
                if !additive && self.point_in_selection_box(tick, key) {
                    // 选中集选框内部：拖动整组（选中集不变）
                    self.editor_state.shape_select.begin_drag(tick, key);
                } else {
                    if !additive {
                        self.editor_state.shape_select.clear_selection();
                    }
                    // 框选按鼠标原始 tick：吸附会把不足一拍的拉框压成一条竖线，
                    // 夹在两个 beat 之间的图形就永远框不中（DRAW-025 S4 实测）。
                    self.editor_state
                        .shape_select
                        .begin_marquee(tick, key, additive);
                }
            }
        }
        // 编辑图形绑定「当前这一个目标」：换选/取消选中后自动失效，
        // 避免下次选中同一对象时凭空回到编辑态。
        if !self.is_shape_edit_armed() {
            self.shape_tools.edit_armed = None;
            self.shape_tools.group_edit = None;
        }
        // 仅叠加层视觉变化（文档未变）：清网格缓存驱动重绘。
        // **不能**用 `mark_notes_changed()`——那会置 notes_changed 并重建空间索引（O(N)）。
    }

    /// 鼠标工具：拖动中 —— 更新预览偏移（拖动整组）或拉框矩形（框选），均不改文档
    pub(crate) fn handle_shape_select_moved(&mut self, tick: f32, key: f32) {
        if self.is_shape_editing() {
            self.update_shape_edit(tick, key);
            return;
        }
        if self.editor_state.shape_select.is_dragging() {
            let snap = self.editor_state.view.snap_precision;
            self.editor_state.shape_select.update_drag(tick, key, snap);
            // 待确认镜像即时落位（owning 预览 / 镜像描边 / 选框共用同一份几何）
            sync_pending_drag_geometry(self);
        } else if self.editor_state.shape_select.is_marqueeing() {
            // 与按下同口径：不吸附，避免把拉框压成竖线
            self.editor_state.shape_select.update_marquee(tick, key);
        }
    }

    /// 鼠标工具：左键释放 —— 有实际位移则提交**整组移动**，否则落在框选上
    pub(crate) fn handle_shape_select_released(&mut self) {
        if self.is_shape_editing() {
            self.finish_shape_edit();
            self.shape_tools.reset_pending_drag_applied();
            return;
        }
        if let Some((dtick, dkey)) = self.editor_state.shape_select.end_drag() {
            move_selected_shapes(self, dtick, dkey);
        } else if let Some(area) = self.editor_state.shape_select.end_marquee() {
            finish_shape_marquee(self, area);
        }
        self.shape_tools.reset_pending_drag_applied();
    }

    // ── 删除 ─────────────────────────────────────────────

    /// 删除**整个选中集**的绘制图形（图形对象 + 它们生成的音符），返回是否删除了内容
    ///
    /// 待确认镜像（未 √ 的几何）只摘几何：无音符 → 不动文档、不记文档历史。
    pub(crate) fn delete_selected_drawn_shape(&mut self) -> bool {
        // 取消任何未完成的拖拽：删除后拖拽若残留，会把 is_editing() 永久卡在 true
        // （进而阻塞撤销/重做）。删除是终结操作，拖拽状态必须一并收敛。
        let _ = self.editor_state.shape_select.end_drag();
        // 选中集快照（ID + 各自音符 + 待确认来源）
        let selected: Vec<(u64, Vec<ShapeNote>, Option<PendingShapeRef>)> = self
            .editor_state
            .shape_select
            .selected_shapes()
            .map(|s| (s.id, s.notes.clone(), s.pending))
            .collect();
        if selected.is_empty() {
            return false;
        }

        // ① 待确认镜像：几何从 owning tool 摘除（含镜像退场与同容器下标前移）
        let mut pending_deleted = false;
        for (_, _, pending) in &selected {
            if pending.is_some_and(|r| remove_pending(self, r)) {
                pending_deleted = true;
            }
        }
        let selected: Vec<(u64, Vec<ShapeNote>)> = selected
            .into_iter()
            .filter_map(|(id, notes, pending)| pending.is_none().then_some((id, notes)))
            .collect();
        if selected.is_empty() {
            return pending_deleted;
        }

        // ② 已确认图形：先按值定位待删索引（按轨分组；`used` 在整组范围内共享，
        //    避免重复认领同一事件）
        let mut per_track: HashMap<usize, (HashSet<usize>, Vec<usize>)> = HashMap::new();
        for (_, notes) in &selected {
            for n in notes {
                let lookup = lumino_editor_state::note_to_event(Note::new(n.tick, n.key, n.length));
                let entry = per_track.entry(n.track).or_default();
                let doc_notes = self.editor_state.data.track_notes(n.track);
                if let Some(idx) = doc_notes.position_of_unused(&lookup, &entry.0) {
                    entry.0.insert(idx);
                    entry.1.push(idx);
                }
            }
        }
        let has_notes = per_track.values().any(|(_, idx)| !idx.is_empty());
        if has_notes {
            // 历史快照必须在变更**之前**入栈（快照 = 变更前状态，撤销才能恢复）
            self.editor_state
                .data
                .push_history_with_op_kind(OpKind::NoteDelete);
        }
        let mut deleted = 0usize;
        for (track, (_, indices)) in &per_track {
            if indices.is_empty() {
                continue;
            }
            deleted += self.editor_state.data.remove_notes_merged(*track, indices);
        }
        let ids: Vec<u64> = selected.into_iter().map(|(id, _)| id).collect();
        if deleted == 0 {
            if has_notes {
                self.editor_state.data.discard_last_history();
            }
            // 音符已不存在（例如用户先手动删了）：仍然收掉图形对象本身
            for id in ids {
                self.editor_state.shape_select.mark_deleted(id);
            }
            return true;
        }
        for id in ids {
            self.editor_state.shape_select.mark_deleted(id);
        }
        self.mark_notes_changed();
        true
    }

    /// 撤销/重做后与文档对账：修正「已删除图形」的可见性
    ///
    /// 删除走 Snapshot 历史——`History::undo` 推入 redo 栈的是**当前状态快照**，
    /// 不携带分组 ID，因此无法用分组对称回放。改为按事实对账：
    /// 「该图形是否还有音符残留在文档里」——方向无关，天然对称。
    /// 只对曾被本功能删除过的图形生效（见 `ShapeSelectState::reconcile_alive`）。
    pub(crate) fn reconcile_drawn_shapes(&mut self) {
        const PROBE_LIMIT: usize = 8;
        let probes: Vec<(u64, bool)> = self
            .editor_state
            .shape_select
            .shapes()
            .iter()
            .map(|s| {
                let alive = s.notes.iter().take(PROBE_LIMIT).any(|n| {
                    let lookup =
                        lumino_editor_state::note_to_event(Note::new(n.tick, n.key, n.length));
                    self.editor_state
                        .data
                        .track_notes(n.track)
                        .position_of(&lookup)
                        .is_some()
                });
                (s.id, alive)
            })
            .collect();
        self.editor_state.shape_select.reconcile_alive(&probes);
    }
}

/// 任意历史条目携带的分组 ID（用于把图形状态与撤销/重做绑定）
pub(crate) fn history_entry_group(entry: &HistoryEntry) -> Option<u64> {
    match entry {
        HistoryEntry::Snapshot(s) => s.group_id,
        HistoryEntry::Operation(o) => o.group_id,
        HistoryEntry::Create(c) => c.group_id,
    }
}

/// 结束框选：拉框足够大时把框内**全部**可见图形并入选中集，返回本次**新**选中的 ID
///
/// - 拉框过小（点一下空白）→ 不改变选中态：按下阶段已（非 `additive` 时）取消选中，
///   保持「点空白取消选中」的既有语义（因此**不能**用逻辑尺寸判定，
///   纵横卷帘下逻辑单位对应的像素尺度不同，统一按屏幕边长比较）。
/// - `area.additive`（起框时按住 Shift）→ 与既有选中集求**并集**；否则**替换**。
/// - 命中口径：图形逻辑外接框与拉框 **AABB 相交**（含接触）。
fn finish_shape_marquee(editor: &mut Editor, area: ShapeMarquee) -> Vec<u64> {
    let (t0, t1, k0, k1) = area.rect();
    let p0 = editor.line_pos_screen_pos((t0, k0));
    let p1 = editor.line_pos_screen_pos((t1, k1));
    if (p1.x - p0.x).abs() < MARQUEE_MIN_PX && (p1.y - p0.y).abs() < MARQUEE_MIN_PX {
        return Vec::new();
    }
    let track = editor.editor_state.data.current_track;
    let inside: Vec<u64> = editor
        .editor_state
        .shape_select
        .visible_on(track)
        .filter(|s| {
            s.source
                .bounds()
                .is_some_and(|b| aabb_overlap(b, (t0, t1, k0, k1)))
        })
        .map(|s| s.id)
        .collect();
    if area.additive {
        // 并集：逐个加入，返回真正新增的
        let mut newly = Vec::new();
        for id in inside {
            if editor.editor_state.shape_select.add_to_selection(id) {
                newly.push(id);
            }
        }
        newly
    } else {
        editor
            .editor_state
            .shape_select
            .select_all_of(inside.clone());
        inside
    }
}

/// 把**整个选中集**的全部音符整体平移（`MoveOp` 删旧 + 加新），并同步平移各图形几何
///
/// 多选下所有图形共用**同一个历史分组**，因此一次撤销 / 重做能整组回放。
/// 待确认镜像（未 √ 的几何）走另一条路：几何写回 owning tool 的待确认容器，
/// 不动文档、不产生历史（音符尚未存在）。
/// 返回是否实际发生了移动。
fn move_selected_shapes(editor: &mut Editor, dtick: f32, dkey: f32) -> bool {
    if dtick == 0.0 && dkey == 0.0 {
        return false;
    }
    // 选中集快照（ID + 各自音符 + 待确认来源），后续改文档时不再借用注册表
    let selected: Vec<(u64, Vec<ShapeNote>, Option<PendingShapeRef>)> = editor
        .editor_state
        .shape_select
        .selected_shapes()
        .map(|s| (s.id, s.notes.clone(), s.pending))
        .collect();
    if selected.is_empty() {
        return false;
    }

    // ① 待确认镜像：几何在拖动过程中已**即时写回**（`sync_pending_drag_geometry`），
    //    此处只补「尚未写回的残差」并为整次移动记一次历史（无文档变更）
    let mut pending_moved = false;
    let residual = editor.shape_tools.take_pending_drag_increment(dtick, dkey);
    for (id, _, pending) in &selected {
        let Some(r) = *pending else {
            continue;
        };
        if let Some((inc_tick, inc_key)) = residual {
            // 残差非零（本函数被直接调用 / 拖动帧未落几何）：补写；引用失效则跳过
            if !translate_pending(editor, r, inc_tick, inc_key) {
                continue;
            }
            if let Some(src) = editor.pending_source(r) {
                editor.editor_state.shape_select.set_source(*id, src);
            }
        }
        record_pending_movement(editor, r);
        pending_moved = true;
    }
    editor.shape_tools.reset_pending_drag_applied();

    // ② 已确认图形：按值定位文档音符 → MoveOp（镜像无音符，天然不参与）
    let max_key = editor.editor_state.view.visible_key_count.saturating_sub(1);
    let dt = dtick.round() as i64;
    let dk = dkey.round() as i32;

    // ②-1 按轨聚合「按值定位用的音符值」（登记时的值语义，与写入同源）
    //    跨图形聚合后由同一份 `used` 集合去重，杜绝两个图形重复认领同一文档事件
    let mut lookups_by_track: HashMap<usize, Vec<NoteEvent>> = HashMap::new();
    for (_, notes, pending) in &selected {
        if pending.is_some() {
            continue;
        }
        for n in notes {
            lookups_by_track
                .entry(n.track)
                .or_default()
                .push(lumino_editor_state::note_to_event(Note::new(
                    n.tick, n.key, n.length,
                )));
        }
    }

    // ②-2 逐轨按值定位真实事件，构造 originals / moved（同值多份按份数分配）
    let mut ops: Vec<MoveOp> = Vec::new();
    let mut seq = 0u16;
    for (track, lookups) in &lookups_by_track {
        let mut used: HashSet<usize> = HashSet::new();
        let mut originals: Vec<NoteEvent> = Vec::new();
        let mut moved: Vec<NoteEvent> = Vec::new();
        for lookup in lookups {
            let Some(idx) = editor
                .editor_state
                .data
                .track_notes(*track)
                .position_of_unused(lookup, &used)
            else {
                continue;
            };
            used.insert(idx);
            let Some(orig) = editor
                .editor_state
                .data
                .track_notes(*track)
                .get(idx)
                .copied()
            else {
                continue;
            };
            let new_tick = (orig.start_tick as i64 + dt).max(0) as u32;
            let new_key = (orig.key as i32 + dk).clamp(0, max_key as i32) as u8;
            let len = orig.end_tick.saturating_sub(orig.start_tick).max(1);
            let mut new_event = NoteEvent::new(
                new_tick,
                new_tick.saturating_add(len),
                new_key,
                orig.velocity,
                orig.channel,
            );
            // release_velocity 需保留（new() 归零，补回）
            new_event.release_velocity = orig.release_velocity;
            originals.push(orig);
            moved.push(new_event);
        }
        if originals.is_empty() {
            continue;
        }
        ops.push(MoveOp {
            track_id: *track as u32,
            originals,
            moved,
            delta_tick: dt as i32,
            delta_key: dk as i16,
            seq,
        });
        seq = seq.saturating_add(1);
    }
    if ops.is_empty() {
        // 仅待确认几何被移动：文档未变 → 不写历史、不重建空间索引
        return pending_moved;
    }

    let modified = editor
        .editor_state
        .data
        .apply_move_ops(&ops, false, max_key);
    if modified == 0 {
        return pending_moved;
    }
    let group = editor.editor_state.data.push_move_op(ops);
    // 整组共用同一分组：几何 / 音符值随组一起平移（待确认镜像已在 ① 写回）
    for (id, _, pending) in selected {
        if pending.is_some() {
            continue;
        }
        editor
            .editor_state
            .shape_select
            .translate_shape(id, Some(group), dtick, dkey);
    }
    editor.mark_notes_changed();
    true
}

/// 删除一件待确认几何（鼠标工具 Delete；无音符 → 不动文档）
///
/// 摘除 owning tool 容器里的对应几何 + 其镜像；同容器中下标更大的镜像引用随之前移
/// （几何未变，无需重建，镜像 ID 与其余选中态保持稳定）。
fn remove_pending(editor: &mut Editor, r: PendingShapeRef) -> bool {
    // 无镜像 ⇒ 该件不在选中域（不该由本路径删除），不做任何改动
    let Some(id) = editor.editor_state.shape_select.pending_mirror(r) else {
        return false;
    };
    let removed = match r {
        PendingShapeRef::Shape(i) => remove_at(&mut editor.editor_state.shape_tool.shapes, i),
        PendingShapeRef::CurvePath(i) => remove_at(&mut editor.editor_state.line_tool.paths, i),
        PendingShapeRef::BrushStroke(i) => {
            remove_at(&mut editor.editor_state.brush_tool.strokes, i)
        }
        PendingShapeRef::Composed(i) => remove_at(&mut editor.editor_state.pending_composed, i),
    };
    if !removed {
        return false;
    }
    editor.editor_state.shape_select.remove(id);
    editor.editor_state.shape_select.shift_pending_indices(r);
    record_pending_movement(editor, r);
    true
}

/// 待确认镜像拖动中的**几何即时写回**（每帧增量，owning tool 保持唯一权威）
///
/// 已确认图形走「幽灵」语义（拖动只预览、松手才落文档），但待确认几何**没有文档层**：
/// 它本身就是待 √ 的产物，直接落位最简单也最不容易分叉——拖动帧里
/// 「owning 预览（曲线/形状 canvas、画刷 wgpu 方块）／镜像描边／选中选框」
/// 读到的永远是同一份几何。
///
/// 增量 = 本次累计偏移 − 已写回偏移（`ShapeDrag` 存的是相对按下点的**累计**偏移）。
fn sync_pending_drag_geometry(editor: &mut Editor) {
    let Some(d) = editor.editor_state.shape_select.drag() else {
        return;
    };
    let Some(inc) = editor
        .shape_tools
        .take_pending_drag_increment(d.delta_tick, d.delta_key)
    else {
        return;
    };
    let mirrors: Vec<(u64, PendingShapeRef)> = editor
        .editor_state
        .shape_select
        .selected_shapes()
        .filter_map(|s| s.pending.map(|r| (s.id, r)))
        .collect();
    if mirrors.is_empty() {
        return;
    }
    for (id, r) in mirrors {
        if translate_pending(editor, r, inc.0, inc.1)
            && let Some(src) = editor.pending_source(r)
        {
            editor.editor_state.shape_select.set_source(id, src);
        }
    }
}

/// 为一次已完成的待确认几何移动补记历史快照（owning tool 的待确认历史）
///
/// 曲线 / 画刷的待确认几何有独立历史（Ctrl+Z 可回退一次移动）；形状工具无
/// 待确认几何历史，无从记录。
fn record_pending_movement(editor: &mut Editor, r: PendingShapeRef) {
    match r {
        PendingShapeRef::CurvePath(_) => editor.editor_state.line_tool.push_path_history(),
        PendingShapeRef::BrushStroke(_) => editor.editor_state.brush_tool.push_path_history(),
        PendingShapeRef::Shape(_) | PendingShapeRef::Composed(_) => {}
    }
}

/// 把一次整体平移**写回** owning tool 的待确认几何（鼠标工具拖动待确认产物）
///
/// 平移口径与拖动已确认图形（[`DrawnShapeSource::translate`]）一致：
/// tick 下限钳 0、key 钳 0..=255。历史快照由调用方在**一次拖动结束时**记一次
/// （见 [`Self::record_pending_movement`]），避免逐帧刷屏。
fn translate_pending(editor: &mut Editor, r: PendingShapeRef, dtick: f32, dkey: f32) -> bool {
    use lumino_editor_state::editor_state::drawn_shape::clamp_translate_delta;
    match r {
        PendingShapeRef::Shape(i) => {
            let Some(s) = editor.editor_state.shape_tool.shapes.get_mut(i) else {
                return false;
            };
            let (min_tick, min_key, max_key) = (
                s.rect.0.min(s.rect.2),
                s.rect.1.min(s.rect.3),
                s.rect.1.max(s.rect.3),
            );
            let (dtick, dkey) = clamp_translate_delta(dtick, dkey, min_tick, min_key, max_key);
            s.rect.0 += dtick;
            s.rect.2 += dtick;
            s.rect.1 += dkey;
            s.rect.3 += dkey;
        }
        PendingShapeRef::CurvePath(i) => {
            let Some(path) = editor.editor_state.line_tool.paths.get_mut(i) else {
                return false;
            };
            let mut min_tick = f32::INFINITY;
            let mut min_key = f32::INFINITY;
            let mut max_key = f32::NEG_INFINITY;
            for a in path.iter() {
                min_tick = min_tick.min(a.pos.0);
                min_key = min_key.min(a.pos.1);
                max_key = max_key.max(a.pos.1);
            }
            let (dtick, dkey) = clamp_translate_delta(dtick, dkey, min_tick, min_key, max_key);
            // 控制柄是**相对**偏移（见 `BezierAnchor::out_handle_abs`），
            // 平移锚点即可整体平移曲线（与曲线工具拖动整条路径同口径）
            for a in path.iter_mut() {
                a.pos = (a.pos.0 + dtick, a.pos.1 + dkey);
            }
        }
        PendingShapeRef::BrushStroke(i) => {
            let Some(s) = editor.editor_state.brush_tool.strokes.get_mut(i) else {
                return false;
            };
            s.translate(dtick, dkey);
        }
        PendingShapeRef::Composed(i) => {
            let Some(c) = editor.editor_state.pending_composed.get_mut(i) else {
                return false;
            };
            c.translate(dtick, dkey);
        }
    }
    true
}

/// 登记一个图形对象（自动绑定当前历史分组）
#[cfg(test)]
fn record_drawn_shape_on(
    editor: &mut Editor,
    track: usize,
    source: DrawnShapeSource,
    notes: Vec<ShapeNote>,
) {
    let group = editor.last_create_group();
    editor
        .editor_state
        .shape_select
        .add(track, group, source, notes);
}

#[cfg(test)]
mod tests;
