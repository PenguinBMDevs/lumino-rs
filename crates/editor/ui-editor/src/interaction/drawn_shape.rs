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

use iced_core::{Point, Rectangle, Size};
use lumino_editor_state::{
    DrawnShapeSource, PendingShapeRef, ShapeMarquee, ShapeNote, shape_tool::point_in_shape,
};
use lumino_message::Tool;
use lumino_midi_loader::NoteEvent;
use lumino_midi_model::TickIndexedEvents;
use lumino_note_core::history::{HistoryEntry, MoveOp, OpKind};

use crate::interaction::line_tool::geom::flatten_path;
use crate::{Editor, Note};

/// 折线命中容差（屏幕像素）：点击位置到笔画折线的最短距离阈值
const HIT_TOLERANCE_PX: f32 = 6.0;

/// 空白拉框的最小屏幕边长（像素）：低于此值视为「一次点击」而非框选
const MARQUEE_MIN_PX: f32 = 4.0;

/// 选中集「选框」相对其几何外接框的四周外扩（像素）
///
/// 让选框略大于图形本身：细线图形（折线）不至于退化成零宽/零高的框，
/// 也让「框内拖动」有一圈可抓取的余量。渲染与命中**共用**本常量，
/// 保证「看到的框」就是「抓得住的框」。
pub(crate) const SHAPE_BOX_PADDING_PX: f32 = 3.0;

/// 单条画刷笔画的登记三元组：(落笔基准轨, 折线点列, 该笔画生成的音符)
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

/// 点到线段的最短距离（同坐标系）
fn point_segment_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let abx = b.0 - a.0;
    let aby = b.1 - a.1;
    let len2 = abx * abx + aby * aby;
    let t = if len2 <= f32::EPSILON {
        0.0
    } else {
        (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / len2).clamp(0.0, 1.0)
    };
    let cx = a.0 + abx * t;
    let cy = a.1 + aby * t;
    ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()
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

    /// 登记一个图形对象（自动绑定当前历史分组）
    fn record_drawn_shape_on(
        &mut self,
        track: usize,
        source: DrawnShapeSource,
        notes: Vec<ShapeNote>,
    ) {
        let group = self.last_create_group();
        self.editor_state.shape_select.add(track, group, source, notes);
    }

    /// 登记形状工具的待确认图形（√ 确认后调用，须在 `clear_pending` 之前）
    ///
    /// `per_shape_notes[i]` 与 `shape_tool.shapes[i]` 一一对应（由 confirm 侧同源产出）。
    pub(crate) fn record_shape_tool_shapes(&mut self, per_shape_notes: Vec<Vec<ShapeNote>>) {
        let track = self.editor_state.data.current_track;
        let sources: Vec<DrawnShapeSource> = self
            .editor_state
            .shape_tool
            .shapes
            .iter()
            .map(|s| DrawnShapeSource::Shape {
                kind: s.kind,
                rect: s.rect,
                shift_constrained: s.shift_constrained,
                filled: s.filled,
            })
            .collect();
        for (source, notes) in sources.into_iter().zip(per_shape_notes) {
            self.record_drawn_shape_on(track, source, notes);
        }
    }

    /// 登记曲线工具的待确认路径（√ 确认后调用，须在 `reset` 之前）
    ///
    /// 每条完整路径（≥ 2 锚点）展开为一条折线；`per_path_notes[i]` 与 `paths[i]` 对应。
    pub(crate) fn record_line_tool_paths(&mut self, per_path_notes: Vec<Vec<ShapeNote>>) {
        let track = self.editor_state.data.current_track;
        let polylines: Vec<Vec<(f32, f32)>> = self
            .editor_state
            .line_tool
            .paths
            .iter()
            .map(|p| {
                flatten_path(p)
                    .into_iter()
                    .map(|(t, k)| (t as f32, k as f32))
                    .collect()
            })
            .collect();
        for (points, notes) in polylines.into_iter().zip(per_path_notes) {
            self.record_drawn_shape_on(track, DrawnShapeSource::Polyline { points }, notes);
        }
    }

    /// 登记画刷工具的待确认笔画（√ 确认后调用，须在 `reset` 之前）
    ///
    /// `per_stroke_notes[i]` 与 `brush_tool.strokes[i]` 对应；图形以**落笔基准轨**
    /// （`BrushStroke::base_track`）登记，保证「在哪个轨画的就在哪个轨选中」。
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
            self.record_drawn_shape_on(track, DrawnShapeSource::Polyline { points }, notes);
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
        if self.current_tool() != Tool::ShapeSelect {
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
    fn pending_source(&self, r: PendingShapeRef) -> Option<DrawnShapeSource> {
        match r {
            PendingShapeRef::Shape(i) => {
                let s = self.editor_state.shape_tool.shapes.get(i)?;
                Some(DrawnShapeSource::Shape {
                    kind: s.kind,
                    rect: s.rect,
                    shift_constrained: s.shift_constrained,
                    filled: s.filled,
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

    /// 待确认镜像拖动中的**几何即时写回**（每帧增量，owning tool 保持唯一权威）
    ///
    /// 已确认图形走「幽灵」语义（拖动只预览、松手才落文档），但待确认几何**没有文档层**：
    /// 它本身就是待 √ 的产物，直接落位最简单也最不容易分叉——拖动帧里
    /// 「owning 预览（曲线/形状 canvas、画刷 wgpu 方块）／镜像描边／选中选框」
    /// 读到的永远是同一份几何。
    ///
    /// 增量 = 本次累计偏移 − 已写回偏移（`ShapeDrag` 存的是相对按下点的**累计**偏移）。
    fn sync_pending_drag_geometry(&mut self) {
        let Some(d) = self.editor_state.shape_select.drag() else {
            return;
        };
        let (applied_tick, applied_key) = self.pending_drag_applied;
        let inc = (d.delta_tick - applied_tick, d.delta_key - applied_key);
        if inc.0 == 0.0 && inc.1 == 0.0 {
            return;
        }
        let mirrors: Vec<(u64, PendingShapeRef)> = self
            .editor_state
            .shape_select
            .selected_shapes()
            .filter_map(|s| s.pending.map(|r| (s.id, r)))
            .collect();
        if mirrors.is_empty() {
            return;
        }
        for (id, r) in mirrors {
            if self.translate_pending(r, inc.0, inc.1)
                && let Some(src) = self.pending_source(r)
            {
                self.editor_state.shape_select.set_source(id, src);
            }
        }
        self.pending_drag_applied = (d.delta_tick, d.delta_key);
    }

    /// 把一次整体平移**写回** owning tool 的待确认几何（鼠标工具拖动待确认产物）
    ///
    /// 平移口径与拖动已确认图形（[`DrawnShapeSource::translate`]）一致：
    /// tick 下限钳 0、key 钳 0..=255。历史快照由调用方在**一次拖动结束时**记一次
    /// （见 [`Self::record_pending_movement`]），避免逐帧刷屏。
    fn translate_pending(&mut self, r: PendingShapeRef, dtick: f32, dkey: f32) -> bool {
        match r {
            PendingShapeRef::Shape(i) => {
                let Some(s) = self.editor_state.shape_tool.shapes.get_mut(i) else {
                    return false;
                };
                s.rect.0 = (s.rect.0 + dtick).max(0.0);
                s.rect.2 = (s.rect.2 + dtick).max(0.0);
                s.rect.1 = (s.rect.1 + dkey).clamp(0.0, 255.0);
                s.rect.3 = (s.rect.3 + dkey).clamp(0.0, 255.0);
            }
            PendingShapeRef::CurvePath(i) => {
                let Some(path) = self.editor_state.line_tool.paths.get_mut(i) else {
                    return false;
                };
                // 控制柄是**相对**偏移（见 `BezierAnchor::out_handle_abs`），
                // 平移锚点即可整体平移曲线（与曲线工具拖动整条路径同口径）
                for a in path.iter_mut() {
                    a.pos = (
                        (a.pos.0 + dtick).max(0.0),
                        (a.pos.1 + dkey).clamp(0.0, 255.0),
                    );
                }
            }
            PendingShapeRef::BrushStroke(i) => {
                let Some(s) = self.editor_state.brush_tool.strokes.get_mut(i) else {
                    return false;
                };
                s.translate(dtick, dkey);
            }
        }
        true
    }

    /// 为一次已完成的待确认几何移动补记历史快照（owning tool 的待确认历史）
    ///
    /// 曲线 / 画刷的待确认几何有独立历史（Ctrl+Z 可回退一次移动）；形状工具无
    /// 待确认几何历史，无从记录。
    fn record_pending_movement(&mut self, r: PendingShapeRef) {
        match r {
            PendingShapeRef::CurvePath(_) => self.editor_state.line_tool.push_path_history(),
            PendingShapeRef::BrushStroke(_) => self.editor_state.brush_tool.push_path_history(),
            PendingShapeRef::Shape(_) => {}
        }
    }

    /// 删除一件待确认几何（鼠标工具 Delete；无音符 → 不动文档）
    ///
    /// 摘除 owning tool 容器里的对应几何 + 其镜像；同容器中下标更大的镜像引用随之前移
    /// （几何未变，无需重建，镜像 ID 与其余选中态保持稳定）。
    fn remove_pending(&mut self, r: PendingShapeRef) -> bool {
        // 无镜像 ⇒ 该件不在选中域（不该由本路径删除），不做任何改动
        let Some(id) = self.editor_state.shape_select.pending_mirror(r) else {
            return false;
        };
        let removed = match r {
            PendingShapeRef::Shape(i) => remove_at(&mut self.editor_state.shape_tool.shapes, i),
            PendingShapeRef::CurvePath(i) => remove_at(&mut self.editor_state.line_tool.paths, i),
            PendingShapeRef::BrushStroke(i) => {
                remove_at(&mut self.editor_state.brush_tool.strokes, i)
            }
        };
        if !removed {
            return false;
        }
        self.editor_state.shape_select.remove(id);
        self.editor_state.shape_select.shift_pending_indices(r);
        self.record_pending_movement(r);
        true
    }

    // ── 命中 ─────────────────────────────────────────────

    /// 命中当前音轨**最上层**（后登记优先）的可见图形，返回其 ID
    pub(crate) fn hit_test_drawn_shape(&self, tick: f32, key: f32) -> Option<u64> {
        let track = self.editor_state.data.current_track;
        let px_per_tick = self.editor_state.view.zoom_x;
        let px_per_key = self.editor_state.view.zoom_y;
        let probe = self.line_pos_screen_pos((tick, key));

        // 逆序遍历：最后登记的图形画在最上层，命中也应优先
        for shape in self.editor_state.shape_select.visible_on(track).rev() {
            let hit = match &shape.source {
                DrawnShapeSource::Shape {
                    kind,
                    rect,
                    shift_constrained,
                    ..
                } => point_in_shape(
                    *kind,
                    *rect,
                    *shift_constrained,
                    px_per_tick,
                    px_per_key,
                    tick,
                    key,
                ),
                DrawnShapeSource::Polyline { points } => polyline_hit(self, probe, points),
            };
            if hit {
                return Some(shape.id);
            }
        }
        None
    }

    /// 屏幕坐标处命中的已绘制图形 ID（供画布右键判定菜单目标）
    pub fn drawn_shape_at_screen(&self, pos: iced_core::Point) -> Option<u64> {
        let tick = self.pos_to_tick(pos);
        let key = self.pos_to_raw_key(pos);
        self.hit_test_drawn_shape(tick, key)
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
    pub(crate) fn selection_box(&self) -> Option<Rectangle> {
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
    pub(crate) fn drag_preview_delta(&self) -> (f32, f32) {
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
        self.grid_cache.clear();
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
        self.pending_drag_applied = (0.0, 0.0);
        // 手势边界自愈：命中判定与选框都建立在镜像几何之上，动手前先保证它是最新的
        // （任何未预料到的待确认几何改动都不会再退化成「框不中」）。
        self.refresh_pending_mirror_geometry();
        match self.hit_test_drawn_shape(tick, key) {
            Some(id) => {
                if additive {
                    self.editor_state.shape_select.toggle_selection(id);
                } else if !self.editor_state.shape_select.is_selected(id) {
                    self.editor_state.shape_select.select_only(id);
                }
                if self.editor_state.shape_select.is_selected(id) {
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
                    let snapped = self.snap_tick(tick);
                    self.editor_state
                        .shape_select
                        .begin_marquee(snapped, key, additive);
                }
            }
        }
        // 仅叠加层视觉变化（文档未变）：清网格缓存驱动重绘。
        // **不能**用 `mark_notes_changed()`——那会置 notes_changed 并重建空间索引（O(N)）。
        self.grid_cache.clear();
    }

    /// 鼠标工具：拖动中 —— 更新预览偏移（拖动整组）或拉框矩形（框选），均不改文档
    pub(crate) fn handle_shape_select_moved(&mut self, tick: f32, key: f32) {
        if self.editor_state.shape_select.is_dragging() {
            let snap = self.editor_state.view.snap_precision;
            self.editor_state.shape_select.update_drag(tick, key, snap);
            // 待确认镜像即时落位（owning 预览 / 镜像描边 / 选框共用同一份几何）
            self.sync_pending_drag_geometry();
        } else if self.editor_state.shape_select.is_marqueeing() {
            let snapped = self.snap_tick(tick);
            self.editor_state.shape_select.update_marquee(snapped, key);
        } else {
            return;
        }
        self.grid_cache.clear();
    }

    /// 鼠标工具：左键释放 —— 有实际位移则提交**整组移动**，否则落在框选上
    pub(crate) fn handle_shape_select_released(&mut self) {
        if let Some((dtick, dkey)) = self.editor_state.shape_select.end_drag() {
            self.move_selected_shapes(dtick, dkey);
        } else if let Some(area) = self.editor_state.shape_select.end_marquee() {
            self.finish_shape_marquee(area);
        }
        self.pending_drag_applied = (0.0, 0.0);
        self.grid_cache.clear();
    }

    /// 结束框选：拉框足够大时把框内**全部**可见图形并入选中集，返回本次**新**选中的 ID
    ///
    /// - 拉框过小（点一下空白）→ 不改变选中态：按下阶段已（非 `additive` 时）取消选中，
    ///   保持「点空白取消选中」的既有语义（因此**不能**用逻辑尺寸判定，
    ///   纵横卷帘下逻辑单位对应的像素尺度不同，统一按屏幕边长比较）。
    /// - `area.additive`（起框时按住 Shift）→ 与既有选中集求**并集**；否则**替换**。
    /// - 命中口径：图形逻辑外接框与拉框 **AABB 相交**（含接触）。
    fn finish_shape_marquee(&mut self, area: ShapeMarquee) -> Vec<u64> {
        let (t0, t1, k0, k1) = area.rect();
        let p0 = self.line_pos_screen_pos((t0, k0));
        let p1 = self.line_pos_screen_pos((t1, k1));
        if (p1.x - p0.x).abs() < MARQUEE_MIN_PX && (p1.y - p0.y).abs() < MARQUEE_MIN_PX {
            return Vec::new();
        }
        let track = self.editor_state.data.current_track;
        let inside: Vec<u64> = self
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
                if self.editor_state.shape_select.add_to_selection(id) {
                    newly.push(id);
                }
            }
            newly
        } else {
            self.editor_state.shape_select.select_all_of(inside.clone());
            inside
        }
    }

    /// 把**整个选中集**的全部音符整体平移（`MoveOp` 删旧 + 加新），并同步平移各图形几何
    ///
    /// 多选下所有图形共用**同一个历史分组**，因此一次撤销 / 重做能整组回放。
    /// 待确认镜像（未 √ 的几何）走另一条路：几何写回 owning tool 的待确认容器，
    /// 不动文档、不产生历史（音符尚未存在）。
    /// 返回是否实际发生了移动。
    fn move_selected_shapes(&mut self, dtick: f32, dkey: f32) -> bool {
        if dtick == 0.0 && dkey == 0.0 {
            return false;
        }
        // 选中集快照（ID + 各自音符 + 待确认来源），后续改文档时不再借用注册表
        let selected: Vec<(u64, Vec<ShapeNote>, Option<PendingShapeRef>)> = self
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
        for (id, _, pending) in &selected {
            let Some(r) = *pending else {
                continue;
            };
            let (applied_tick, applied_key) = self.pending_drag_applied;
            let inc = (dtick - applied_tick, dkey - applied_key);
            if inc.0 != 0.0 || inc.1 != 0.0 {
                // 残差非零（本函数被直接调用 / 拖动帧未落几何）：补写；引用失效则跳过
                if !self.translate_pending(r, inc.0, inc.1) {
                    continue;
                }
                if let Some(src) = self.pending_source(r) {
                    self.editor_state.shape_select.set_source(*id, src);
                }
            }
            self.record_pending_movement(r);
            pending_moved = true;
        }
        self.pending_drag_applied = (0.0, 0.0);

        // ② 已确认图形：按值定位文档音符 → MoveOp（镜像无音符，天然不参与）
        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
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
                let Some(idx) = self
                    .editor_state
                    .data
                    .track_notes(*track)
                    .position_of_unused(lookup, &used)
                else {
                    continue;
                };
                used.insert(idx);
                let Some(orig) = self.editor_state.data.track_notes(*track).get(idx).copied()
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

        let modified = self.editor_state.data.apply_move_ops(&ops, false, max_key);
        if modified == 0 {
            return pending_moved;
        }
        let group = self.editor_state.data.push_move_op(ops);
        // 整组共用同一分组：几何 / 音符值随组一起平移（待确认镜像已在 ① 写回）
        for (id, _, pending) in selected {
            if pending.is_some() {
                continue;
            }
            self.editor_state
                .shape_select
                .translate_shape(id, Some(group), dtick, dkey);
        }
        self.mark_notes_changed();
        true
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
            if pending.is_some_and(|r| self.remove_pending(r)) {
                pending_deleted = true;
            }
        }
        let selected: Vec<(u64, Vec<ShapeNote>)> = selected
            .into_iter()
            .filter_map(|(id, notes, pending)| pending.is_none().then_some((id, notes)))
            .collect();
        if selected.is_empty() {
            self.grid_cache.clear();
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
            deleted += self
                .editor_state
                .data
                .remove_notes_merged(*track, indices);
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
            self.grid_cache.clear();
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

/// 折线命中：探测点到折线各段的屏幕距离是否 ≤ 容差
fn polyline_hit(editor: &Editor, probe: iced_core::Point, points: &[(f32, f32)]) -> bool {
    if points.is_empty() {
        return false;
    }
    let screen: Vec<(f32, f32)> = points
        .iter()
        .map(|&(t, k)| {
            let p = editor.line_pos_screen_pos((t, k));
            (p.x, p.y)
        })
        .collect();
    let probe = (probe.x, probe.y);
    if screen.len() == 1 {
        let d = ((probe.0 - screen[0].0).powi(2) + (probe.1 - screen[0].1).powi(2)).sqrt();
        return d <= HIT_TOLERANCE_PX;
    }
    screen
        .windows(2)
        .any(|seg| point_segment_distance(probe, seg[0], seg[1]) <= HIT_TOLERANCE_PX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::test_helpers::seed_notes;
    use lumino_editor_state::{BezierAnchor, BrushStroke, ShapeKind, ShapeToolInteraction};
    use lumino_message::Tool;

    /// 构造非 Conductor 轨（track 1）编辑器，吸附精度 = 1
    fn test_editor() -> Editor {
        let mut editor = Editor::new();
        seed_notes(&mut editor, 2, 1, &[]);
        editor.editor_state.view.snap_precision = 1.0;
        editor
    }

    /// 拉出并确认一个 0..4 × 60..64 的矩形轮廓（16 条音符），返回编辑器
    fn draw_and_confirm_rect() -> Editor {
        let mut editor = test_editor();
        editor.set_shape(ShapeKind::Rectangle);
        editor.editor_state.shape_tool.fill_enabled = false;
        editor.handle_shape_tool_pressed(0.0, 60.0, false);
        editor.handle_shape_tool_moved(4.0, 64.0);
        editor.handle_shape_tool_released();
        assert!(editor.confirm_shape_tool(), "矩形应生成音符");
        editor
    }

    fn note_keys(editor: &Editor) -> Vec<u16> {
        editor
            .editor_state
            .data
            .current_track_notes()
            .iter()
            .map(|n| n.key as u16)
            .collect()
    }

    fn note_ticks(editor: &Editor) -> Vec<u32> {
        editor
            .editor_state
            .data
            .current_track_notes()
            .iter()
            .map(|n| n.start_tick)
            .collect()
    }

    // ── 登记 ─────────────────────────────────────────────

    #[test]
    fn test_confirm_shape_registers_drawn_shape_with_notes() {
        let editor = draw_and_confirm_rect();
        let st = &editor.editor_state.shape_select;
        assert_eq!(st.len(), 1, "确认后应登记 1 个图形对象");
        let shape = &st.shapes()[0];
        assert_eq!(shape.track, 1, "登记在当前音轨");
        assert!(shape.group.is_some(), "应绑定音符创建历史分组");
        assert_eq!(shape.notes.len(), 16, "轮廓矩形 = 5×5 - 3×3 = 16 格");
        assert!(shape.notes.iter().all(|n| n.track == 1));
        assert!(shape.notes.iter().all(|n| n.length == 1.0), "定长 = snap");
        match &shape.source {
            DrawnShapeSource::Shape {
                kind,
                rect,
                shift_constrained,
                filled,
            } => {
                assert_eq!(*kind, ShapeKind::Rectangle);
                assert_eq!(*rect, (0.0, 60.0, 4.0, 64.0));
                assert!(!shift_constrained && !filled);
            }
            other => panic!("期望 Shape 几何，实际 {other:?}"),
        }
    }

    #[test]
    fn test_confirm_curve_registers_polyline_with_notes() {
        let mut editor = test_editor();
        editor.editor_state.line_tool.paths = vec![vec![
            BezierAnchor::new((0.0, 60.0)),
            BezierAnchor::new((240.0, 64.0)),
        ]];
        editor.editor_state.line_tool.recompute_auto_handles();
        assert!(editor.confirm_line_tool(), "曲线应生成音符");
        let st = &editor.editor_state.shape_select;
        assert_eq!(st.len(), 1, "一条路径 = 一个图形对象");
        match &st.shapes()[0].source {
            DrawnShapeSource::Polyline { points } => {
                assert!(points.len() >= 2, "展平折线至少 2 点，实际 {}", points.len());
            }
            other => panic!("期望 Polyline 几何，实际 {other:?}"),
        }
        assert!(!st.shapes()[0].notes.is_empty(), "路径音符应被登记");
    }

    #[test]
    fn test_record_brush_strokes_keeps_base_track_and_notes() {
        let mut editor = test_editor();
        editor.editor_state.brush_tool.strokes.push(BrushStroke {
            points: vec![(0.0, 60.0), (4.0, 62.0)],
            base_track: 1,
        });
        let notes = vec![vec![ShapeNote {
            track: 1,
            tick: 0.0,
            key: 60,
            length: 1.0,
        }]];
        editor.record_brush_strokes(notes);
        let st = &editor.editor_state.shape_select;
        assert_eq!(st.len(), 1);
        assert_eq!(st.shapes()[0].track, 1, "应登记在落笔基准轨");
        assert_eq!(st.shapes()[0].notes.len(), 1);
        assert!(matches!(
            st.shapes()[0].source,
            DrawnShapeSource::Polyline { .. }
        ));
    }

    // ── 点选 ─────────────────────────────────────────────

    #[test]
    fn test_mouse_tool_selects_inside_and_deselects_outside() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        editor.handle_shape_select_pressed(2.0, 62.0, false);
        assert!(
            editor.editor_state.shape_select.selection_len() > 0,
            "点在图形内部应选中"
        );
        editor.handle_shape_select_pressed(900.0, 10.0, false);
        assert_eq!(
            editor.editor_state.shape_select.selection_len(),
            0,
            "点空白应取消选中"
        );
    }

    #[test]
    fn test_circle_hit_uses_ellipse_interior() {
        let mut editor = test_editor();
        editor.set_shape(ShapeKind::Circle);
        editor.editor_state.shape_tool.fill_enabled = false;
        editor.handle_shape_tool_pressed(0.0, 60.0, false);
        editor.handle_shape_tool_moved(4.0, 64.0);
        editor.handle_shape_tool_released();
        assert!(editor.confirm_shape_tool());
        editor.set_tool(Tool::ShapeSelect);
        // 命中测试口径：椭圆内部，而非外接矩形
        assert_eq!(
            editor.hit_test_drawn_shape(2.0, 62.0),
            Some(0),
            "圆心应命中"
        );
        assert_eq!(
            editor.hit_test_drawn_shape(0.05, 60.05),
            None,
            "外接矩形角落（椭圆之外）不应命中"
        );
        editor.handle_shape_select_pressed(2.0, 62.0, false);
        assert!(editor.editor_state.shape_select.selection_len() > 0);
    }

    #[test]
    fn test_other_track_shape_not_hit() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        editor.editor_state.data.current_track = 2;
        editor.handle_shape_select_pressed(2.0, 62.0, false);
        assert_eq!(editor.editor_state.shape_select.selection_len(), 0);
    }

    // ── 移动 ─────────────────────────────────────────────

    #[test]
    fn test_drag_moves_shape_geometry_and_notes() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        // 按下图形内一点并拖到 (+4 tick, +2 key)
        editor.handle_shape_select_pressed(2.0, 62.0, false);
        editor.handle_shape_select_moved(6.0, 64.0);
        editor.handle_shape_select_released();

        let st = &editor.editor_state.shape_select;
        match &st.shapes()[0].source {
            DrawnShapeSource::Shape { rect, .. } => {
                assert_eq!(*rect, (4.0, 62.0, 8.0, 66.0), "几何应整体平移 (+4, +2)")
            }
            other => panic!("期望 Shape，实际 {other:?}"),
        }
        assert!(st.shapes()[0].notes.iter().all(|n| n.key >= 62));
        assert_eq!(st.shapes()[0].moves.len(), 1, "应记录一次移动");
        // 文档音符随之平移：数量不变、key 全部 >= 62、最小起点 = 4
        assert_eq!(editor.editor_state.data.current_track_note_count(), 16);
        assert!(note_keys(&editor).iter().all(|&k| k >= 62));
        assert_eq!(note_ticks(&editor).iter().copied().min(), Some(4));
    }

    #[test]
    fn test_undo_move_restores_geometry_and_notes() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        editor.handle_shape_select_pressed(2.0, 62.0, false);
        editor.handle_shape_select_moved(6.0, 64.0);
        editor.handle_shape_select_released();
        assert!(editor.undo(), "应能撤销移动");
        match &editor.editor_state.shape_select.shapes()[0].source {
            DrawnShapeSource::Shape { rect, .. } => {
                assert_eq!(*rect, (0.0, 60.0, 4.0, 64.0), "撤销后几何应复原")
            }
            other => panic!("期望 Shape，实际 {other:?}"),
        }
        assert!(note_keys(&editor).contains(&60), "撤销后音符应回原位");
        assert_eq!(note_ticks(&editor).iter().copied().min(), Some(0));
    }

    #[test]
    fn test_drag_without_movement_creates_no_history() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        editor.handle_shape_select_pressed(2.0, 62.0, false);
        // 未超过一格：delta 归零
        editor.handle_shape_select_moved(2.2, 62.0);
        editor.handle_shape_select_released();
        assert!(editor.editor_state.shape_select.shapes()[0].moves.is_empty());
        assert_eq!(editor.editor_state.data.current_track_note_count(), 16);
    }

    // ── 删除 ─────────────────────────────────────────────

    #[test]
    fn test_delete_shape_removes_its_notes_and_object() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        editor.handle_shape_select_pressed(2.0, 62.0, false);
        assert!(editor.delete_selected_drawn_shape());
        assert_eq!(
            editor.editor_state.data.current_track_note_count(),
            0,
            "删除图形应连带删除其音符"
        );
        assert_eq!(editor.editor_state.shape_select.visible_on(1).count(), 0);
        assert_eq!(editor.editor_state.shape_select.selection_len(), 0);
    }

    #[test]
    fn test_undo_delete_restores_notes_and_shape() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        editor.handle_shape_select_pressed(2.0, 62.0, false);
        assert!(editor.delete_selected_drawn_shape());
        assert!(editor.undo(), "应能撤销删除");
        assert_eq!(
            editor.editor_state.data.current_track_note_count(),
            16,
            "撤销删除应恢复音符"
        );
        assert_eq!(
            editor.editor_state.shape_select.visible_on(1).count(),
            1,
            "撤销删除应恢复图形可见性"
        );
        assert!(editor.redo(), "应能重做删除");
        assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
        assert_eq!(editor.editor_state.shape_select.visible_on(1).count(), 0);
    }

    #[test]
    fn test_delete_without_notes_still_removes_object() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        editor.handle_shape_select_pressed(2.0, 62.0, false);
        // 先手动清空该轨音符（模拟用户已先删掉音符）
        editor.editor_state.data.document = None;
        assert!(editor.delete_selected_drawn_shape(), "无音符时应仍收掉图形对象");
        assert_eq!(editor.editor_state.shape_select.visible_on(1).count(), 0);
    }

    // ── 历史同步 ──────────────────────────────────────────

    #[test]
    fn test_undo_creation_hides_shape_and_redo_restores() {
        let mut editor = draw_and_confirm_rect();
        assert_eq!(editor.editor_state.shape_select.visible_on(1).count(), 1);
        assert!(editor.undo(), "撤销音符创建应成功");
        assert_eq!(
            editor.editor_state.shape_select.visible_on(1).count(),
            0,
            "撤销后图形应隐藏（避免幽灵描边）"
        );
        assert!(editor.redo(), "重做应成功");
        assert_eq!(editor.editor_state.shape_select.visible_on(1).count(), 1);
    }

    #[test]
    fn test_hidden_shape_cannot_be_hit() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        editor.handle_shape_select_pressed(2.0, 62.0, false);
        // 原地松手（未移动）：仅选中，不产生移动历史
        editor.handle_shape_select_released();
        assert!(editor.editor_state.shape_select.selection_len() > 0);
        assert!(editor.undo());
        assert_eq!(editor.hit_test_drawn_shape(2.0, 62.0), None);
        assert_eq!(editor.editor_state.shape_select.selection_len(), 0);
    }

    #[test]
    fn test_reset_clears_registry() {
        let mut editor = draw_and_confirm_rect();
        assert!(!editor.editor_state.shape_select.is_empty());
        editor.editor_state.reset();
        assert!(editor.editor_state.shape_select.is_empty(), "reset 应清空图形注册表");
    }

    #[test]
    fn test_point_segment_distance_basics() {
        let d = point_segment_distance((5.0, 3.0), (0.0, 0.0), (10.0, 0.0));
        assert!((d - 3.0).abs() < 1e-4, "期望 3.0，实际 {d}");
        let d = point_segment_distance((14.0, 0.0), (0.0, 0.0), (10.0, 0.0));
        assert!((d - 4.0).abs() < 1e-4, "期望 4.0，实际 {d}");
    }

    // ── 切换到鼠标工具：产物保留、不生成音符（回归） ─────────

    /// 拉出（但不 √）一个待确认形状：模拟「用户刚画完图案还没确认」的状态
    fn draw_pending_rect(editor: &mut Editor) {
        editor.set_tool(Tool::Shape);
        editor.set_shape(ShapeKind::Rectangle);
        editor.editor_state.shape_tool.fill_enabled = false;
        editor.handle_shape_tool_pressed(0.0, 60.0, false);
        editor.handle_shape_tool_moved(4.0, 64.0);
        editor.handle_shape_tool_released();
        assert!(
            editor.editor_state.shape_tool.has_pending(),
            "前置条件：应存在待确认图形"
        );
    }

    #[test]
    fn test_switch_to_mouse_tool_keeps_pending_shape_without_notes() {
        let mut editor = test_editor();
        draw_pending_rect(&mut editor);
        editor.set_tool(Tool::ShapeSelect);
        // ① 切换工具**不得**生成音符：用户没按 √，音符就不该落地
        assert_eq!(
            editor.editor_state.data.current_track_note_count(),
            0,
            "切换到鼠标工具不得直接生成音符"
        );
        // ② 待确认图形（图案）必须保留
        assert!(
            editor.editor_state.shape_tool.has_pending(),
            "切换工具不得丢弃待确认图形"
        );
        // ③ 切到鼠标工具：待确认几何**进入图形选中域**（镜像），但**不带音符**——
        //    「没按 √ 就没有音符」与「看得见就必须框得中」两条同时成立。
        //    （BUG 历史：只保留不登记 ⇒ 用户看到图案却框不中、拖不动、无选框。）
        assert_eq!(
            editor.editor_state.shape_select.len(),
            1,
            "待确认图形应登记为选中域镜像（否则鼠标工具下框不中）"
        );
        let mirror = &editor.editor_state.shape_select.shapes()[0];
        assert!(
            mirror.pending.is_some(),
            "该条目必须是待确认镜像（来源 = 形状工具的待确认图形）"
        );
        assert!(
            mirror.notes.is_empty(),
            "镜像不得携带音符：音符只属于 √ 确认"
        );
        assert!(
            mirror.source.bounds().is_some(),
            "镜像必须有几何（可供框选命中与画选框）"
        );
        assert!(
            editor.selection_box().is_none(),
            "尚未框选 / 点选时不该直接出现选框"
        );
        // ④ 离开鼠标工具：镜像撤掉（其余工具由 owning 预览渲染，不该留选中域条目）
        editor.set_tool(Tool::Pencil);
        assert!(
            editor.editor_state.shape_select.is_empty(),
            "离开鼠标工具应撤掉镜像"
        );
        // ⑤ 切回形状工具仍能 √ 固化 —— 图案没被毁
        editor.set_tool(Tool::Shape);
        assert!(editor.confirm_shape_tool(), "切回后仍可 √ 固化");
        assert_eq!(editor.editor_state.data.current_track_note_count(), 16);
        assert_eq!(editor.editor_state.shape_select.visible_on(1).count(), 1);
    }

    #[test]
    fn test_switch_to_mouse_tool_keeps_pending_curve_paths() {
        let mut editor = test_editor();
        editor.set_tool(Tool::Curve);
        editor.editor_state.line_tool.paths = vec![vec![
            BezierAnchor::new((0.0, 60.0)),
            BezierAnchor::new((240.0, 64.0)),
        ]];
        editor.editor_state.line_tool.recompute_auto_handles();
        editor.set_tool(Tool::ShapeSelect);
        assert_eq!(
            editor.editor_state.line_tool.paths.len(),
            1,
            "切换工具不得丢弃待确认曲线路径"
        );
        assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
        // 切回曲线工具仍可 √ 生成音符
        editor.set_tool(Tool::Curve);
        assert!(editor.confirm_line_tool());
        assert!(editor.editor_state.data.current_track_note_count() > 0);
    }

    #[test]
    fn test_switch_to_mouse_tool_keeps_pending_brush_strokes() {
        let mut editor = test_editor();
        editor.set_tool(Tool::Brush);
        // 粗细度 1：单层笔画全部落在基准轨，便于断言
        editor.brush.set_thickness(1);
        editor.editor_state.brush_tool.strokes.push(BrushStroke {
            points: vec![(0.0, 60.0), (1.0, 60.0), (2.0, 60.0)],
            base_track: 1,
        });
        editor.set_tool(Tool::ShapeSelect);
        assert_eq!(
            editor.editor_state.brush_tool.strokes.len(),
            1,
            "切换工具不得丢弃待确认笔画"
        );
        assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
        editor.set_tool(Tool::Brush);
        assert!(editor.confirm_brush());
        assert!(editor.editor_state.data.current_track_note_count() > 0);
    }

    #[test]
    fn test_switch_to_any_tool_keeps_pending() {
        // 「其他工具同理」：切到任意工具都保留产物（清空只发生在显式 × / √）
        let mut editor = test_editor();
        draw_pending_rect(&mut editor);
        for tool in [Tool::Curve, Tool::Brush, Tool::Pencil, Tool::Text, Tool::Pointer] {
            editor.set_tool(tool);
            assert!(
                editor.editor_state.shape_tool.has_pending(),
                "切到 {tool:?} 后待确认图形应保留"
            );
            assert_eq!(
                editor.editor_state.data.current_track_note_count(),
                0,
                "切到 {tool:?} 不得生成音符"
            );
        }
    }

    #[test]
    fn test_switch_tool_cancels_interaction_but_keeps_artifacts() {
        // 手势（未完成的拖动）必须收敛；产物必须保留 —— 两者不可混为一谈
        let mut editor = test_editor();
        draw_pending_rect(&mut editor);
        editor.editor_state.shape_tool.interaction = ShapeToolInteraction::Dragging {
            start: (0.0, 60.0),
        };
        editor.set_tool(Tool::ShapeSelect);
        assert_eq!(
            editor.editor_state.shape_tool.interaction,
            ShapeToolInteraction::None,
            "切换工具应收敛未完成的交互手势"
        );
        assert!(
            editor.editor_state.shape_tool.has_pending(),
            "但待确认图形必须留着"
        );
    }

    #[test]
    fn test_switch_tool_cancels_shape_drag_without_moving_it() {
        // 拖动已绘制图形途中切走：预览偏移丢弃（几何/音符不动），
        // 拖拽态必须收敛，否则 is_editing() 永久 true → 撤销/重做被静默堵死
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        editor.handle_shape_select_pressed(2.0, 62.0, false);
        editor.handle_shape_select_moved(6.0, 64.0);
        assert!(editor.editor_state.shape_select.is_dragging());
        editor.set_tool(Tool::Curve);
        assert!(
            !editor.editor_state.shape_select.is_dragging(),
            "换工具必须收敛拖拽态"
        );
        assert!(!editor.is_editing(), "拖拽态残留会让 is_editing() 永久为 true");
        match &editor.editor_state.shape_select.shapes()[0].source {
            DrawnShapeSource::Shape { rect, .. } => {
                assert_eq!(*rect, (0.0, 60.0, 4.0, 64.0), "几何不应被平移")
            }
            other => panic!("期望 Shape，实际 {other:?}"),
        }
        assert_eq!(editor.editor_state.data.current_track_note_count(), 16);
    }

    #[test]
    fn test_pending_preview_visible_rule() {
        let mut editor = test_editor();
        editor.set_tool(Tool::Shape);
        assert!(
            editor.pending_preview_visible(Tool::Shape),
            "拥有者工具下应可见"
        );
        assert!(
            !editor.pending_preview_visible(Tool::Curve),
            "非拥有者绘制工具下不应渲染"
        );
        editor.set_tool(Tool::ShapeSelect);
        for owner in [Tool::Curve, Tool::Shape, Tool::Brush, Tool::Text] {
            assert!(
                editor.pending_preview_visible(owner),
                "鼠标工具下 {owner:?} 的待确认预览应可见（图案不消失）"
            );
        }
        editor.set_tool(Tool::Pencil);
        assert!(
            !editor.pending_preview_visible(Tool::Curve),
            "铅笔工具下不渲染待确认预览（避免浮层干扰音符编辑）"
        );
    }

    // ── 空白拉框（框选） ──────────────────────────────────

    #[test]
    fn test_marquee_selects_shape_inside_box() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        // 空白处按下（远离 0..4 × 60..64 的图形）→ 拉出覆盖图形的框
        editor.handle_shape_select_pressed(50.0, 80.0, false);
        assert!(
            editor.editor_state.shape_select.is_marqueeing(),
            "空白按下应进入拉框态"
        );
        editor.handle_shape_select_moved(0.0, 55.0);
        editor.handle_shape_select_released();
        assert_eq!(
            editor.editor_state.shape_select.selected_ids(),
            &[0],
            "框内图形应被框选选中"
        );
        assert!(
            !editor.editor_state.shape_select.is_marqueeing(),
            "松手后拉框态应清空"
        );
    }

    #[test]
    fn test_marquee_outside_box_selects_nothing() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        editor.handle_shape_select_pressed(2.0, 62.0, false); // 先选中图形
        assert!(editor.editor_state.shape_select.selection_len() > 0);
        // 空白处拉一个与图形不相交的框 → 取消选中
        editor.handle_shape_select_pressed(50.0, 80.0, false);
        editor.handle_shape_select_moved(60.0, 90.0);
        editor.handle_shape_select_released();
        assert_eq!(
            editor.editor_state.shape_select.selection_len(),
            0,
            "框外无图形应保持未选中"
        );
    }

    #[test]
    fn test_tiny_drag_is_click_not_marquee() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        editor.handle_shape_select_pressed(2.0, 62.0, false);
        assert!(editor.editor_state.shape_select.selection_len() > 0);
        // 空白处「点一下」（无实际位移）→ 仍是「点空白取消选中」，不构成框选
        editor.handle_shape_select_pressed(50.0, 80.0, false);
        editor.handle_shape_select_moved(50.0, 80.0);
        editor.handle_shape_select_released();
        assert_eq!(editor.editor_state.shape_select.selection_len(), 0);
    }

    #[test]
    fn test_marquee_selects_all_shapes_inside_box() {
        // 多选语义：框选把框内**全部**图形一并选中（不再只取最上层）
        let mut editor = draw_and_confirm_rect();
        // 再登记一个不重叠的远侧矩形
        let far = editor.editor_state.shape_select.add(
            1,
            None,
            DrawnShapeSource::Shape {
                kind: ShapeKind::Rectangle,
                rect: (200.0, 60.0, 204.0, 64.0),
                shift_constrained: false,
                filled: false,
            },
            Vec::new(),
        );
        editor.set_tool(Tool::ShapeSelect);
        // 拉一个同时覆盖两个图形的框
        editor.handle_shape_select_pressed(300.0, 100.0, false);
        editor.handle_shape_select_moved(0.0, 50.0);
        editor.handle_shape_select_released();
        assert_eq!(
            editor.editor_state.shape_select.selected_ids(),
            &[0, far],
            "框内全部图形都应被选中（按登记顺序）"
        );
    }

    #[test]
    fn test_marquee_ignores_hidden_shape_and_other_track() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        // 切到别的音轨：当前轨无图形 → 框选不应选中任何东西
        editor.editor_state.data.current_track = 2;
        editor.handle_shape_select_pressed(50.0, 80.0, false);
        editor.handle_shape_select_moved(0.0, 55.0);
        editor.handle_shape_select_released();
        assert_eq!(editor.editor_state.shape_select.selection_len(), 0);
    }

    // ── 选中选框 + 框内拖动（统一移动方式） ─────────────────

    /// 一条大 L 形折线（逻辑 0..500 × 60..80）+ 与之匹配的文档音符，
    /// 并已完成选中。选框内部有大量「远离折线」的空白，用于验证「框内拖动」。
    fn editor_with_selected_polyline() -> Editor {
        let mut editor = Editor::new();
        seed_notes(
            &mut editor,
            2,
            1,
            &[
                Note::new(0.0, 60, 1.0),
                Note::new(500.0, 60, 1.0),
                Note::new(500.0, 80, 1.0),
            ],
        );
        editor.editor_state.view.snap_precision = 1.0;
        let points = vec![(0.0, 60.0), (500.0, 60.0), (500.0, 80.0)];
        let notes = vec![
            ShapeNote {
                track: 1,
                tick: 0.0,
                key: 60,
                length: 1.0,
            },
            ShapeNote {
                track: 1,
                tick: 500.0,
                key: 60,
                length: 1.0,
            },
            ShapeNote {
                track: 1,
                tick: 500.0,
                key: 80,
                length: 1.0,
            },
        ];
        let id = editor.editor_state.shape_select.add(
            1,
            None,
            DrawnShapeSource::Polyline { points },
            notes,
        );
        editor.set_tool(Tool::ShapeSelect);
        editor.editor_state.shape_select.select_only(id);
        editor
    }

    #[test]
    fn test_selection_box_exists_and_padding() {
        let editor = test_editor();
        assert!(
            editor.selection_box().is_none(),
            "无选中时不应有选框"
        );

        let editor2 = editor_with_selected_polyline();
        let rect = editor2.selection_box().expect("选中后应有选框");
        assert!(rect.width > 0.0 && rect.height > 0.0, "选框必须有面积");
        // 折线逻辑 0..500 × 60..80 → 屏幕外接框按 padding 外扩
        let a = editor2.line_pos_screen_pos((0.0, 60.0));
        let b = editor2.line_pos_screen_pos((500.0, 80.0));
        let min_x = a.x.min(b.x) - SHAPE_BOX_PADDING_PX;
        assert!(
            (rect.position().x - min_x).abs() < 1e-3,
            "选框左边界应为几何外接框左边界外扩 padding"
        );

        // 图形属 track 1，切到 track 2 后不应再有选框
        let mut editor3 = editor_with_selected_polyline();
        editor3.editor_state.data.current_track = 2;
        assert!(
            editor3.selection_box().is_none(),
            "图形不属当前音轨时不应有选框"
        );
    }

    #[test]
    fn test_press_inside_selection_box_keeps_selection_and_start_drag() {
        let mut editor = editor_with_selected_polyline();
        // (250, 70) 位于折线选框内部，但远离折线本身（点选命中应为 None）
        assert_eq!(editor.hit_test_drawn_shape(250.0, 70.0), None);
        assert!(editor.point_in_selection_box(250.0, 70.0), "该点应在选框内");
        let id = editor.editor_state.shape_select.selected_ids().to_vec();
        editor.handle_shape_select_pressed(250.0, 70.0, false);
        assert_eq!(
            editor.editor_state.shape_select.selected_ids(),
            &id[..],
            "点选框内部不应取消选中"
        );
        assert!(editor.editor_state.shape_select.is_dragging(), "应进入拖动");
        // 未移动即松手：不产生移动，选中保持
        editor.handle_shape_select_released();
        assert_eq!(editor.editor_state.shape_select.selected_ids(), &id[..]);
        assert!(editor.editor_state.shape_select.shapes()[0].moves.is_empty());
    }

    #[test]
    fn test_drag_inside_selection_box_moves_polyline_and_notes() {
        let mut editor = editor_with_selected_polyline();
        // 框内空白处按下并拖动 (+10 tick, +2 key)
        editor.handle_shape_select_pressed(250.0, 70.0, false);
        editor.handle_shape_select_moved(260.0, 72.0);
        editor.handle_shape_select_released();

        match &editor.editor_state.shape_select.shapes()[0].source {
            DrawnShapeSource::Polyline { points } => assert_eq!(
                points,
                &vec![(10.0, 62.0), (510.0, 62.0), (510.0, 82.0)],
                "折线几何应整体平移 (+10, +2)"
            ),
            other => panic!("期望 Polyline，实际 {other:?}"),
        }
        assert_eq!(
            editor.editor_state.shape_select.shapes()[0].moves.len(),
            1,
            "应记录一次移动"
        );
        // 文档音符随之平移
        let mut keys: Vec<u16> = note_keys(&editor);
        keys.sort_unstable();
        assert_eq!(keys, vec![62, 62, 82], "音符应整体上移 2 个半音");
        let mut ticks: Vec<u32> = note_ticks(&editor);
        ticks.sort_unstable();
        assert_eq!(ticks, vec![10, 510, 510], "音符应整体右移 10 tick");

        // 撤销 → 几何与音符复原
        assert!(editor.undo(), "应能撤销框内拖动产生的移动");
        match &editor.editor_state.shape_select.shapes()[0].source {
            DrawnShapeSource::Polyline { points } => assert_eq!(
                points,
                &vec![(0.0, 60.0), (500.0, 60.0), (500.0, 80.0)],
                "撤销后几何应复原"
            ),
            other => panic!("期望 Polyline，实际 {other:?}"),
        }
        assert!(note_keys(&editor).contains(&60));
    }

    #[test]
    fn test_point_hit_takes_priority_over_selection_box() {
        let mut editor = editor_with_selected_polyline();
        let poly_id = editor
            .editor_state
            .shape_select
            .selected_ids()
            .first()
            .copied()
            .expect("前置：折线已选中");
        // 在折线选框内部再登记一个小矩形（后登记 = 最上层）
        let rect_id = editor.editor_state.shape_select.add(
            1,
            None,
            DrawnShapeSource::Shape {
                kind: ShapeKind::Rectangle,
                rect: (240.0, 68.0, 260.0, 72.0),
                shift_constrained: false,
                filled: false,
            },
            Vec::new(),
        );
        assert!(
            editor.point_in_selection_box(250.0, 70.0),
            "前置：该点确在折线选框内"
        );
        editor.handle_shape_select_pressed(250.0, 70.0, false);
        assert_eq!(
            editor.editor_state.shape_select.selected_ids(),
            &[rect_id],
            "点中图形应优先于「选框内部拖动」（否则无法选中重叠的其它图形）"
        );
        assert!(
            editor.editor_state.shape_select.is_dragging(),
            "拖动应落在被命中的那个图形上（选中集已替换为它）"
        );
        editor.handle_shape_select_released();
        assert_eq!(editor.editor_state.shape_select.selected_ids(), &[rect_id]);
        assert_ne!(rect_id, poly_id, "两个图形应各有独立 ID");
    }

    #[test]
    fn test_press_outside_selection_box_clears_and_starts_marquee() {
        let mut editor = editor_with_selected_polyline();
        // (900, 10) 落在选框外 → 取消选中并起框
        assert!(!editor.point_in_selection_box(900.0, 10.0));
        editor.handle_shape_select_pressed(900.0, 10.0, false);
        assert_eq!(
            editor.editor_state.shape_select.selection_len(),
            0,
            "选框外按下应取消选中"
        );
        assert!(
            editor.editor_state.shape_select.is_marqueeing(),
            "选框外按下应起框"
        );
        assert!(!editor.editor_state.shape_select.is_dragging());
        editor.handle_shape_select_released();
    }

    #[test]
    fn test_selection_box_follows_drag_preview_offset() {
        let mut editor = editor_with_selected_polyline();
        let base = editor.selection_box().expect("应有选框");
        editor.handle_shape_select_pressed(250.0, 70.0, false);
        editor.handle_shape_select_moved(260.0, 72.0);
        let dragged = editor.selection_box().expect("拖拽中仍应有选框");
        // 拖拽预览：选框应随预览偏移移动（未落文档）
        let dx = dragged.position().x - base.position().x;
        let dy = dragged.position().y - base.position().y;
        assert!(
            (dx - 10.0 * editor.editor_state.view.zoom_x).abs() < 1e-3,
            "选框 X 应跟随预览偏移，实际 dx={dx}"
        );
        assert!((dy + 2.0 * editor.editor_state.view.zoom_y).abs() < 1e-3, "选框 Y 应跟随预览偏移（key 增大 y 减小），实际 dy={dy}");
        editor.handle_shape_select_released();
    }

    #[test]
    fn test_press_just_outside_box_top_is_not_grabbed() {
        // 真实按下链路（`handle_tool_pressed`）必须用**未取整**的原始 key：
        // 选框命中区精细到 3px，若按键位整数化（zoom_y=20 时 1 key = 20px），
        // 框外 5px 的点会被量化进框内、被误判成「框内拖动」而不是起框。
        let mut editor = editor_with_selected_polyline();
        let top_y = editor.line_pos_screen_pos((0.0, 80.0)).y - SHAPE_BOX_PADDING_PX;
        let x = editor.line_pos_screen_pos((250.0, 70.0)).x;
        let pos = iced_core::Point::new(x, top_y - 5.0);
        let snapped = editor.snap_tick(editor.pos_to_tick(pos));
        editor.handle_tool_pressed(pos, false, snapped, 0);
        assert!(
            editor.editor_state.shape_select.is_marqueeing(),
            "框外按下应起框，而非被整数 key 量化误判为框内拖动"
        );
        assert_eq!(editor.editor_state.shape_select.selection_len(), 0);
        assert!(!editor.editor_state.shape_select.is_dragging());
    }

    // ── 多选（框选多个 / Shift 增删 / 整组移动 · 删除） ──────

    /// 两条带音符的水平折线（track 1）+ 匹配的文档音符，且**已全部选中**
    ///
    /// A：key 60 的横线（0..300）；B：key 70 的横线（0..300），共 4 条文档音符。
    /// 两条线相距 10 个半音 → 选框并集 0..300 × 60..70，中间有大片空白可拖。
    fn editor_with_two_selected_polylines() -> Editor {
        let mut editor = Editor::new();
        seed_notes(
            &mut editor,
            2,
            1,
            &[
                Note::new(0.0, 60, 1.0),
                Note::new(100.0, 60, 1.0),
                Note::new(0.0, 70, 1.0),
                Note::new(100.0, 70, 1.0),
            ],
        );
        editor.editor_state.view.snap_precision = 1.0;
        let line = |key: u16| {
            DrawnShapeSource::Polyline {
                points: vec![(0.0, key as f32), (300.0, key as f32)],
            }
        };
        let notes = |key: u16| {
            vec![
                ShapeNote {
                    track: 1,
                    tick: 0.0,
                    key,
                    length: 1.0,
                },
                ShapeNote {
                    track: 1,
                    tick: 100.0,
                    key,
                    length: 1.0,
                },
            ]
        };
        let a = editor
            .editor_state
            .shape_select
            .add(1, None, line(60), notes(60));
        let b = editor
            .editor_state
            .shape_select
            .add(1, None, line(70), notes(70));
        editor.set_tool(Tool::ShapeSelect);
        editor.editor_state.shape_select.select_all_of([a, b]);
        editor
    }

    #[test]
    fn test_shift_click_toggles_selection_membership() {
        let mut editor = editor_with_two_selected_polylines();
        let ids: Vec<u64> = editor.editor_state.shape_select.selected_ids().to_vec();
        assert_eq!(ids.len(), 2);
        // Shift 点中**已选中**的成员 → 从选中集移出，且不进入拖拽
        editor.handle_shape_select_pressed(150.0, 60.0, true);
        assert_eq!(
            editor.editor_state.shape_select.selected_ids(),
            &ids[1..],
            "Shift 点击已选中者应把它移出选中集"
        );
        assert!(
            !editor.editor_state.shape_select.is_dragging(),
            "移出后不应进入拖拽"
        );
        // 再 Shift 点它 → 加回（追加到末尾）
        editor.handle_shape_select_pressed(150.0, 60.0, true);
        assert_eq!(
            editor.editor_state.shape_select.selected_ids(),
            &[ids[1], ids[0]],
            "再次 Shift 应加回选中集"
        );
        assert!(editor.editor_state.shape_select.is_dragging());
        editor.handle_shape_select_released();
    }

    #[test]
    fn test_shift_marquee_unions_instead_of_replacing() {
        let mut editor = editor_with_two_selected_polylines();
        // 先只选中第一条（清空选中集后点它）
        editor.editor_state.shape_select.clear_selection();
        editor.handle_shape_select_pressed(150.0, 60.0, false);
        assert_eq!(editor.editor_state.shape_select.selection_len(), 1);
        // Shift 拉框覆盖两条线 → 与既有选中集求**并集**
        editor.handle_shape_select_pressed(0.0, 58.0, true);
        editor.handle_shape_select_moved(300.0, 72.0);
        editor.handle_shape_select_released();
        assert_eq!(
            editor.editor_state.shape_select.selection_len(),
            2,
            "Shift 框选应并入既有选中集"
        );
        // 普通拉框（不按 Shift）→ **替换**选中集
        editor.handle_shape_select_pressed(0.0, 58.0, false);
        editor.handle_shape_select_moved(300.0, 62.0);
        editor.handle_shape_select_released();
        assert_eq!(
            editor.editor_state.shape_select.selection_len(),
            1,
            "普通框选应替换选中集"
        );
    }

    #[test]
    fn test_click_on_selected_member_drags_whole_group() {
        let mut editor = editor_with_two_selected_polylines();
        let ids: Vec<u64> = editor.editor_state.shape_select.selected_ids().to_vec();
        // 点中**已选中**的成员：选中集不变 → 拖动即整组移动
        editor.handle_shape_select_pressed(150.0, 60.0, false);
        assert_eq!(
            editor.editor_state.shape_select.selected_ids(),
            &ids[..],
            "点已选中的成员不应缩小选中集"
        );
        editor.handle_shape_select_moved(160.0, 62.0);
        editor.handle_shape_select_released();
        let st = &editor.editor_state.shape_select;
        for (i, expect_key) in [(0usize, 62.0f32), (1usize, 72.0f32)] {
            match &st.shapes()[i].source {
                DrawnShapeSource::Polyline { points } => {
                    assert_eq!(points[0], (10.0, expect_key), "第 {i} 条线应整组平移");
                    assert_eq!(points[1], (310.0, expect_key));
                }
                other => panic!("期望 Polyline，实际 {other:?}"),
            }
            assert_eq!(st.shapes()[i].moves.len(), 1, "应记录一次移动");
        }
        let mut keys: Vec<u16> = note_keys(&editor);
        keys.sort_unstable();
        assert_eq!(keys, vec![62, 62, 72, 72], "两组音符都应上移 2 个半音");
    }

    #[test]
    fn test_drag_inside_selection_box_moves_whole_selection() {
        let mut editor = editor_with_two_selected_polylines();
        // (150, 65) 位于两条线之间：不命中任何图形，但在并集选框内部
        assert_eq!(
            editor.hit_test_drawn_shape(150.0, 65.0),
            None,
            "前置：并集中间是空白"
        );
        assert!(editor.point_in_selection_box(150.0, 65.0));
        editor.handle_shape_select_pressed(150.0, 65.0, false);
        editor.handle_shape_select_moved(160.0, 67.0);
        editor.handle_shape_select_released();

        let mut ticks: Vec<u32> = note_ticks(&editor);
        ticks.sort_unstable();
        assert_eq!(ticks, vec![10, 10, 110, 110], "整组音符应右移 10 tick");
        let mut keys: Vec<u16> = note_keys(&editor);
        keys.sort_unstable();
        assert_eq!(keys, vec![62, 62, 72, 72]);
        // 整组共用同一历史分组 → 一次撤销全回来
        assert!(editor.undo(), "应能撤销整组移动");
        let mut keys: Vec<u16> = note_keys(&editor);
        keys.sort_unstable();
        assert_eq!(keys, vec![60, 60, 70, 70], "撤销应整组还原");
        match &editor.editor_state.shape_select.shapes()[0].source {
            DrawnShapeSource::Polyline { points } => {
                assert_eq!(points[0], (0.0, 60.0), "撤销后几何应复原")
            }
            other => panic!("期望 Polyline，实际 {other:?}"),
        }
    }

    #[test]
    fn test_delete_removes_whole_selection() {
        let mut editor = editor_with_two_selected_polylines();
        assert!(editor.delete_selected_drawn_shape());
        assert_eq!(
            editor.editor_state.data.current_track_note_count(),
            0,
            "删除应连带删掉整组的全部音符"
        );
        assert_eq!(editor.editor_state.shape_select.visible_on(1).count(), 0);
        assert_eq!(editor.editor_state.shape_select.selection_len(), 0);
        // 一次快照 → 一次撤销恢复整组
        assert!(editor.undo(), "一次撤销应恢复整组");
        assert_eq!(editor.editor_state.data.current_track_note_count(), 4);
        assert_eq!(editor.editor_state.shape_select.visible_on(1).count(), 2);
        assert!(editor.redo(), "重做应再次删除整组");
        assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
    }

    #[test]
    fn test_right_click_on_selected_member_keeps_multi_selection() {
        let mut editor = editor_with_two_selected_polylines();
        let ids: Vec<u64> = editor.editor_state.shape_select.selected_ids().to_vec();
        assert_eq!(ids.len(), 2);
        // 右键命中**已在选中集内**的成员 → 保持多选（菜单「删除」作用于整个选区）
        let id = editor
            .editor_state
            .shape_select
            .shapes()
            .first()
            .map(|s| s.id)
            .expect("应有图形");
        editor.select_drawn_shape(id);
        assert_eq!(
            editor.editor_state.shape_select.selected_ids(),
            &ids[..],
            "右键已选中成员不应缩小选中集"
        );
    }
}
