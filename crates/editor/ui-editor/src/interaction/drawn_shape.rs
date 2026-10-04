//! 图形选中工具（音符画工具栏「鼠标工具」）交互 —— 点选 / 拖动移动 / 删除
//!
//! 与「登记」两面一体：
//! - **登记**：曲线 / 形状 / 画刷工具 √ 确认生成音符后，把各自的矢量几何 + 它生成的
//!   音符登记进 `EditorState::shape_select`（见 `record_*` 方法），并绑定本次音符
//!   创建历史的分组 ID，供撤销/重做同步（见 `Editor::undo/redo`）。
//! - **点选**：鼠标工具（`Tool::ShapeSelect`）左键按下 —— 命中当前音轨**最上层**
//!   的可见图形则选中（高亮描边由 `grid::drawn_shape_box` 渲染），点空白取消选中。
//! - **框选**：鼠标工具在空白处按下并拖动 → 叠加层实时画出拉框，松手后选中框内
//!   **最上层**的可见图形（见 [`Editor::finish_shape_marquee`]）。拉框过小视为一次
//!   普通点击，不改变选中态（保持「点空白取消选中」语义）。
//! - **拖动移动**：按住选中图形拖动 → 实时预览偏移（叠加层）→ 松手后把该图形生成的
//!   音符整体按 `MoveOp`（删旧 + 加新）平移，并同步平移几何；历史可撤销/重做。
//! - **删除**：Delete 键或右键菜单「删除」→ 快照历史 + 按值删除该图形的全部音符 +
//!   标记图形已删除；撤销该删除可恢复。
//!
//! 进入鼠标工具时，当前绘制工具的**待确认内容会被 √ 固化**（见
//! `Editor::commit_pending_drawing`）——否则 `EditorState::set_tool` 的「切换工具 = ×」
//! 会把用户刚画好的图案连同登记机会一起丢掉，表现为「切到选择工具图案就消失」。
//!
//! 命中判定（逻辑坐标 → 屏幕像素空间，容差统一为像素）：
//! - 形状工具图形：直接复用 `point_in_shape`（含 Shift 正图形约束与圆形内部判定）；
//! - 折线（曲线展平路径 / 画刷笔画）：点到各段的屏幕距离 ≤ [`HIT_TOLERANCE_PX`]。
//!
//! 图形是**叠加对象**（音符才是真正的文档内容）：几何与音符列表登记在注册表，
//! 移动/删除改的是 document（走既有历史/渲染管线）。

use std::collections::{HashMap, HashSet};

use lumino_editor_state::{DrawnShapeSource, ShapeMarquee, ShapeNote, shape_tool::point_in_shape};
use lumino_midi_loader::NoteEvent;
use lumino_midi_model::TickIndexedEvents;
use lumino_note_core::history::{HistoryEntry, MoveOp, OpKind};

use crate::interaction::line_tool::geom::flatten_path;
use crate::{Editor, Note};

/// 折线命中容差（屏幕像素）：点击位置到笔画折线的最短距离阈值
const HIT_TOLERANCE_PX: f32 = 6.0;

/// 空白拉框的最小屏幕边长（像素）：低于此值视为「一次点击」而非框选
const MARQUEE_MIN_PX: f32 = 4.0;

/// 单条画刷笔画的登记三元组：(落笔基准轨, 折线点列, 该笔画生成的音符)
type StrokeRecord = (usize, Vec<(f32, f32)>, Vec<ShapeNote>);

/// 逻辑 AABB 相交（含接触）；参数布局统一为 `(min_tick, max_tick, min_key, max_key)`
fn aabb_overlap(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    a.0 <= b.1 && b.0 <= a.1 && a.2 <= b.3 && b.2 <= a.3
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

    /// 选中指定绘制图形（供右键菜单等外部入口）；`None` = 取消选中
    pub fn select_drawn_shape(&mut self, id: Option<u64>) {
        self.editor_state.shape_select.select(id);
        self.grid_cache.clear();
    }

    // ── 点选 / 拖动 ───────────────────────────────────────

    /// 鼠标工具：左键按下 —— 命中则选中并起拖；点空白则取消选中并**起框**（框选）
    pub(crate) fn handle_shape_select_pressed(&mut self, tick: f32, key: f32) {
        let hit = self.hit_test_drawn_shape(tick, key);
        self.editor_state.shape_select.select(hit);
        if let Some(id) = hit {
            self.editor_state.shape_select.begin_drag(id, tick, key);
        } else {
            // 空白按下：起框。是否真的构成框选由松手时的屏幕尺度判定，
            // 因此「点一下空白取消选中」的既有语义不受影响。
            let snapped = self.snap_tick(tick);
            self.editor_state.shape_select.begin_marquee(snapped, key);
        }
        // 仅叠加层视觉变化（文档未变）：清网格缓存驱动重绘。
        // **不能**用 `mark_notes_changed()`——那会置 notes_changed 并重建空间索引（O(N)）。
        self.grid_cache.clear();
    }

    /// 鼠标工具：拖动中 —— 更新预览偏移（拖动图形）或拉框矩形（框选），均不改文档
    pub(crate) fn handle_shape_select_moved(&mut self, tick: f32, key: f32) {
        if self.editor_state.shape_select.is_dragging() {
            let snap = self.editor_state.view.snap_precision;
            self.editor_state.shape_select.update_drag(tick, key, snap);
        } else if self.editor_state.shape_select.is_marqueeing() {
            let snapped = self.snap_tick(tick);
            self.editor_state.shape_select.update_marquee(snapped, key);
        } else {
            return;
        }
        self.grid_cache.clear();
    }

    /// 鼠标工具：左键释放 —— 有实际位移则提交移动，否则落在框选上
    pub(crate) fn handle_shape_select_released(&mut self) {
        if let Some((id, dtick, dkey)) = self.editor_state.shape_select.end_drag() {
            self.move_drawn_shape(id, dtick, dkey);
        } else if let Some(area) = self.editor_state.shape_select.end_marquee() {
            self.finish_shape_marquee(area);
        }
        self.grid_cache.clear();
    }

    /// 结束框选：拉框足够大时选中框内**最上层**的可见图形，返回其 ID
    ///
    /// - 拉框过小（点一下空白）→ 不改变选中态：按下阶段已取消选中，
    ///   保持「点空白取消选中」的既有语义（因此**不能**用逻辑尺寸判定，
    ///   纵横卷帘下逻辑单位对应的像素尺度不同，统一按屏幕边长比较）。
    /// - 单选模型的折中：框内命中多个图形时取「最后登记」（与点选的最上层口径一致）。
    fn finish_shape_marquee(&mut self, area: ShapeMarquee) -> Option<u64> {
        let (t0, t1, k0, k1) = area.rect();
        let p0 = self.line_pos_screen_pos((t0, k0));
        let p1 = self.line_pos_screen_pos((t1, k1));
        if (p1.x - p0.x).abs() < MARQUEE_MIN_PX && (p1.y - p0.y).abs() < MARQUEE_MIN_PX {
            return None;
        }
        let track = self.editor_state.data.current_track;
        let hit = self
            .editor_state
            .shape_select
            .visible_on(track)
            .rev()
            .find(|s| {
                s.source
                    .bounds()
                    .is_some_and(|b| aabb_overlap(b, (t0, t1, k0, k1)))
            })
            .map(|s| s.id);
        self.editor_state.shape_select.select(hit);
        hit
    }

    /// 把某图形的全部音符整体平移（`MoveOp` 删旧 + 加新），并同步平移几何
    ///
    /// 返回是否实际发生了移动。
    fn move_drawn_shape(&mut self, id: u64, dtick: f32, dkey: f32) -> bool {
        if dtick == 0.0 && dkey == 0.0 {
            return false;
        }
        let Some(shape) = self
            .editor_state
            .shape_select
            .shapes()
            .iter()
            .find(|s| s.id == id)
            .cloned()
        else {
            return false;
        };
        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
        let dt = dtick.round() as i64;
        let dk = dkey.round() as i32;

        // ① 按轨分组「按值定位用的音符值」（登记时的值语义，与写入同源）
        let mut lookups_by_track: HashMap<usize, Vec<NoteEvent>> = HashMap::new();
        for n in &shape.notes {
            lookups_by_track
                .entry(n.track)
                .or_default()
                .push(lumino_editor_state::note_to_event(Note::new(
                    n.tick, n.key, n.length,
                )));
        }

        // ② 逐轨按值定位真实事件，构造 originals / moved（同值多份按份数分配）
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
            return false;
        }

        let modified = self.editor_state.data.apply_move_ops(&ops, false, max_key);
        if modified == 0 {
            return false;
        }
        let group = self.editor_state.data.push_move_op(ops);
        self.editor_state
            .shape_select
            .translate_shape(id, Some(group), dtick, dkey);
        self.mark_notes_changed();
        true
    }

    // ── 删除 ─────────────────────────────────────────────

    /// 删除当前选中的绘制图形（图形对象 + 它生成的音符），返回是否删除了内容
    pub(crate) fn delete_selected_drawn_shape(&mut self) -> bool {
        // 取消任何未完成的拖拽：删除后拖拽若残留，会把 is_editing() 永久卡在 true
        // （进而阻塞撤销/重做）。删除是终结操作，拖拽状态必须一并收敛。
        let _ = self.editor_state.shape_select.end_drag();
        let Some(shape) = self.editor_state.shape_select.selected_shape().cloned() else {
            return false;
        };
        // 先按值定位待删索引（按轨分组）
        let mut per_track: HashMap<usize, (HashSet<usize>, Vec<usize>)> = HashMap::new();
        for n in &shape.notes {
            let lookup = lumino_editor_state::note_to_event(Note::new(n.tick, n.key, n.length));
            let entry = per_track.entry(n.track).or_default();
            let notes = self.editor_state.data.track_notes(n.track);
            if let Some(idx) = notes.position_of_unused(&lookup, &entry.0) {
                entry.0.insert(idx);
                entry.1.push(idx);
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
        if deleted == 0 {
            if has_notes {
                self.editor_state.data.discard_last_history();
            }
            // 音符已不存在（例如用户先手动删了）：仍然收掉图形对象本身
            self.editor_state.shape_select.mark_deleted(shape.id);
            self.grid_cache.clear();
            return true;
        }
        self.editor_state.shape_select.mark_deleted(shape.id);
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
    use lumino_editor_state::{BezierAnchor, BrushStroke, ShapeKind};
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
        editor.handle_shape_select_pressed(2.0, 62.0);
        assert!(
            editor.editor_state.shape_select.selected().is_some(),
            "点在图形内部应选中"
        );
        editor.handle_shape_select_pressed(900.0, 10.0);
        assert_eq!(
            editor.editor_state.shape_select.selected(),
            None,
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
        editor.handle_shape_select_pressed(2.0, 62.0);
        assert!(editor.editor_state.shape_select.selected().is_some());
        editor.handle_shape_select_pressed(0.05, 60.05);
        assert_eq!(editor.editor_state.shape_select.selected(), None);
    }

    #[test]
    fn test_other_track_shape_not_hit() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        editor.editor_state.data.current_track = 2;
        editor.handle_shape_select_pressed(2.0, 62.0);
        assert_eq!(editor.editor_state.shape_select.selected(), None);
    }

    // ── 移动 ─────────────────────────────────────────────

    #[test]
    fn test_drag_moves_shape_geometry_and_notes() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        // 按下图形内一点并拖到 (+4 tick, +2 key)
        editor.handle_shape_select_pressed(2.0, 62.0);
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
        editor.handle_shape_select_pressed(2.0, 62.0);
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
        editor.handle_shape_select_pressed(2.0, 62.0);
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
        editor.handle_shape_select_pressed(2.0, 62.0);
        assert!(editor.delete_selected_drawn_shape());
        assert_eq!(
            editor.editor_state.data.current_track_note_count(),
            0,
            "删除图形应连带删除其音符"
        );
        assert_eq!(editor.editor_state.shape_select.visible_on(1).count(), 0);
        assert_eq!(editor.editor_state.shape_select.selected(), None);
    }

    #[test]
    fn test_undo_delete_restores_notes_and_shape() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        editor.handle_shape_select_pressed(2.0, 62.0);
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
        editor.handle_shape_select_pressed(2.0, 62.0);
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
        editor.handle_shape_select_pressed(2.0, 62.0);
        // 原地松手（未移动）：仅选中，不产生移动历史
        editor.handle_shape_select_released();
        assert!(editor.editor_state.shape_select.selected().is_some());
        assert!(editor.undo());
        assert_eq!(editor.hit_test_drawn_shape(2.0, 62.0), None);
        assert_eq!(editor.editor_state.shape_select.selected(), None);
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

    // ── 切换到鼠标工具：图案不得消失（回归） ───────────────

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
    fn test_switch_to_mouse_tool_commits_pending_shape() {
        let mut editor = test_editor();
        draw_pending_rect(&mut editor);
        // 切到「鼠标工具」（框选/选择绘制图形）
        editor.set_tool(Tool::ShapeSelect);
        // 图案不能凭空消失：应视为 √ 固化 —— 音符落地 + 图形登记 + 可命中
        assert_eq!(
            editor.editor_state.data.current_track_note_count(),
            16,
            "切换到鼠标工具应把待确认形状固化为音符（而不是丢弃）"
        );
        assert_eq!(
            editor.editor_state.shape_select.visible_on(1).count(),
            1,
            "固化后图形应已登记，可被选中"
        );
        assert!(
            editor.hit_test_drawn_shape(2.0, 62.0).is_some(),
            "固化后图案应可命中"
        );
        editor.handle_shape_select_pressed(2.0, 62.0);
        editor.handle_shape_select_released();
        assert!(
            editor.editor_state.shape_select.selected().is_some(),
            "固化后图案应可被鼠标工具选中"
        );
    }

    #[test]
    fn test_switch_to_mouse_tool_commits_pending_curve() {
        let mut editor = test_editor();
        editor.set_tool(Tool::Curve);
        editor.editor_state.line_tool.paths = vec![vec![
            BezierAnchor::new((0.0, 60.0)),
            BezierAnchor::new((240.0, 64.0)),
        ]];
        editor.editor_state.line_tool.recompute_auto_handles();
        editor.set_tool(Tool::ShapeSelect);
        assert_eq!(
            editor.editor_state.shape_select.visible_on(1).count(),
            1,
            "切换到鼠标工具应固化待确认曲线并登记图形"
        );
        assert!(editor.editor_state.data.current_track_note_count() > 0);
    }

    #[test]
    fn test_switch_to_mouse_tool_commits_pending_brush() {
        let mut editor = test_editor();
        editor.set_tool(Tool::Brush);
        // 粗细度 1：单层笔画全部落在基准轨，便于断言
        editor.brush.set_thickness(1);
        editor.editor_state.brush_tool.strokes.push(BrushStroke {
            points: vec![(0.0, 60.0), (1.0, 60.0), (2.0, 60.0)],
            base_track: 1,
        });
        editor.set_tool(Tool::ShapeSelect);
        assert!(
            editor.editor_state.data.current_track_note_count() > 0,
            "切换到鼠标工具应固化待确认笔画"
        );
        assert_eq!(editor.editor_state.shape_select.visible_on(1).count(), 1);
    }

    #[test]
    fn test_switch_to_other_drawing_tool_still_discards_pending() {
        // 反向约束：只有进入「鼠标工具」才是 √；切到其它绘制工具仍是「×」
        let mut editor = test_editor();
        draw_pending_rect(&mut editor);
        editor.set_tool(Tool::Curve);
        assert_eq!(
            editor.editor_state.data.current_track_note_count(),
            0,
            "切到其它绘制工具应保持既有「×」语义（丢弃待确认内容）"
        );
        assert_eq!(editor.editor_state.shape_select.visible_on(1).count(), 0);
    }

    #[test]
    fn test_switch_to_mouse_tool_without_pending_is_noop() {
        let mut editor = test_editor();
        editor.set_tool(Tool::Shape);
        editor.set_tool(Tool::ShapeSelect);
        assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
        assert!(editor.editor_state.shape_select.is_empty());
    }

    // ── 空白拉框（框选） ──────────────────────────────────

    #[test]
    fn test_marquee_selects_shape_inside_box() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        // 空白处按下（远离 0..4 × 60..64 的图形）→ 拉出覆盖图形的框
        editor.handle_shape_select_pressed(50.0, 80.0);
        assert!(
            editor.editor_state.shape_select.is_marqueeing(),
            "空白按下应进入拉框态"
        );
        editor.handle_shape_select_moved(0.0, 55.0);
        editor.handle_shape_select_released();
        assert_eq!(
            editor.editor_state.shape_select.selected(),
            Some(0),
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
        editor.handle_shape_select_pressed(2.0, 62.0); // 先选中图形
        assert!(editor.editor_state.shape_select.selected().is_some());
        // 空白处拉一个与图形不相交的框 → 取消选中
        editor.handle_shape_select_pressed(50.0, 80.0);
        editor.handle_shape_select_moved(60.0, 90.0);
        editor.handle_shape_select_released();
        assert_eq!(
            editor.editor_state.shape_select.selected(),
            None,
            "框外无图形应保持未选中"
        );
    }

    #[test]
    fn test_tiny_drag_is_click_not_marquee() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        editor.handle_shape_select_pressed(2.0, 62.0);
        assert!(editor.editor_state.shape_select.selected().is_some());
        // 空白处「点一下」（无实际位移）→ 仍是「点空白取消选中」，不构成框选
        editor.handle_shape_select_pressed(50.0, 80.0);
        editor.handle_shape_select_moved(50.0, 80.0);
        editor.handle_shape_select_released();
        assert_eq!(editor.editor_state.shape_select.selected(), None);
    }

    #[test]
    fn test_marquee_picks_topmost_shape() {
        let mut editor = draw_and_confirm_rect();
        // 再登记一个大框（全部覆盖），后登记者应视为最上层
        editor.editor_state.shape_select.add(
            1,
            None,
            DrawnShapeSource::Shape {
                kind: ShapeKind::Rectangle,
                rect: (0.0, 50.0, 40.0, 90.0),
                shift_constrained: false,
                filled: false,
            },
            Vec::new(),
        );
        editor.set_tool(Tool::ShapeSelect);
        editor.handle_shape_select_pressed(100.0, 100.0);
        editor.handle_shape_select_moved(0.0, 40.0);
        editor.handle_shape_select_released();
        assert_eq!(
            editor.editor_state.shape_select.selected(),
            Some(1),
            "框内命中多个图形时应取最上层（最后登记）"
        );
    }

    #[test]
    fn test_marquee_ignores_hidden_shape_and_other_track() {
        let mut editor = draw_and_confirm_rect();
        editor.set_tool(Tool::ShapeSelect);
        // 切到别的音轨：当前轨无图形 → 框选不应选中任何东西
        editor.editor_state.data.current_track = 2;
        editor.handle_shape_select_pressed(50.0, 80.0);
        editor.handle_shape_select_moved(0.0, 55.0);
        editor.handle_shape_select_released();
        assert_eq!(editor.editor_state.shape_select.selected(), None);
    }
}
