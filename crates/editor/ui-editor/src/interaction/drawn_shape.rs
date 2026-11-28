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

mod editor_pending;
mod editor_reconcile;
mod editor_selection;

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
    let pending_moved = write_back_pending_residual(editor, &selected, dtick, dkey);

    // ② 已确认图形：按值定位文档音符 → MoveOp（镜像无音符，天然不参与）
    let max_key = editor.editor_state.view.visible_key_count.saturating_sub(1);
    let dt = dtick.round() as i64;
    let dk = dkey.round() as i32;

    // ②-1 按轨聚合「按值定位用的音符值」（登记时的值语义，与写入同源）
    //    跨图形聚合后由同一份 `used` 集合去重，杜绝两个图形重复认领同一文档事件
    let lookups_by_track = collect_move_lookups(&selected);

    // ②-2 逐轨按值定位真实事件，构造 originals / moved（同值多份按份数分配）
    let ops = build_move_ops(editor, &lookups_by_track, max_key, dt, dk);
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

/// ① 待确认镜像的残差写回：补写尚未落到 owning tool 的几何增量，并记一次待确认历史
///
/// 返回是否有待确认几何被移动（此时文档可能完全没变）。
fn write_back_pending_residual(
    editor: &mut Editor,
    selected: &[(u64, Vec<ShapeNote>, Option<PendingShapeRef>)],
    dtick: f32,
    dkey: f32,
) -> bool {
    let mut pending_moved = false;
    let residual = editor.shape_tools.take_pending_drag_increment(dtick, dkey);
    for (id, _, pending) in selected {
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
    pending_moved
}

/// ②-1 按轨聚合「按值定位用的音符值」：待确认镜像没有文档音符，天然被跳过
fn collect_move_lookups(
    selected: &[(u64, Vec<ShapeNote>, Option<PendingShapeRef>)],
) -> HashMap<usize, Vec<NoteEvent>> {
    let mut lookups_by_track: HashMap<usize, Vec<NoteEvent>> = HashMap::new();
    for (_, notes, pending) in selected {
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
    lookups_by_track
}

/// ②-2 逐轨按值定位真实事件，构造 originals / moved（同值多份按份数分配）
fn build_move_ops(
    editor: &Editor,
    lookups_by_track: &HashMap<usize, Vec<NoteEvent>>,
    max_key: u16,
    dt: i64,
    dk: i32,
) -> Vec<MoveOp> {
    let mut ops: Vec<MoveOp> = Vec::new();
    let mut seq = 0u16;
    for (track, lookups) in lookups_by_track {
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
    ops
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
