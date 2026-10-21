//! 形状提交与撤销测试（S0-3b 验收：改参数 → undo → 形状与音符**同时**回退）

use crate::Editor;
use crate::tests::test_helpers::seed_notes;
use lumino_core::shape::{
    ShapeFill, ShapeGeometry, ShapeKind, ShapeModel, ShapeOp, ShapeStyle, TumourParams,
};

/// 构造带文档的编辑器：`document` 非空才可写入音符（`insert_note_with_id` 空文档返回 None）
fn editor_with_track(track: usize) -> Editor {
    let mut editor = Editor::new();
    seed_notes(&mut editor, track + 1, track, &[]);
    editor
}

fn straight_shape(track: u16, tick_hi: f32) -> ShapeModel {
    let mut model = ShapeModel::new(0, track, ShapeKind::Line, 0.25);
    model.geometry = ShapeGeometry::from_points([(0.0, 60.0), (tick_hi, 72.0)]);
    model.validate().expect("测试形状应合法");
    model
}

fn closed_shape(fill: ShapeFill, gate_beats: f32) -> ShapeModel {
    let mut geometry =
        ShapeGeometry::from_points([(0.0, 60.0), (480.0, 60.0), (480.0, 64.0), (0.0, 64.0)]);
    geometry.closed = true;
    let mut model = ShapeModel::new(0, 1, ShapeKind::Custom, 0.25);
    model.geometry = geometry;
    model.style = ShapeStyle {
        fill,
        gate_beats,
        ..ShapeStyle::default()
    };
    model
}

#[test]
fn test_fill_shape_is_one_note_per_row() {
    let editor = editor_with_track(1);
    let notes = editor.shape_notes_of(&closed_shape(ShapeFill::Fill, 0.25), 1);
    // 现有 row_spans 的边界口径会把上边界行 64 也算进来（与曲线填充一致）
    assert_eq!(notes.len(), 5, "矩形 60..64 当前填 60..64 五行");
    assert_eq!(
        notes.iter().map(|n| n.key).collect::<Vec<_>>(),
        vec![60, 61, 62, 63, 64]
    );
    assert!(notes.iter().all(|n| (n.length - 480.0).abs() < 1e-3));
}

#[test]
fn test_spam_shape_uses_gate() {
    let editor = editor_with_track(1);
    let gate = 0.0625 * f32::from(editor.editor_state.view.ppq);
    let notes = editor.shape_notes_of(&closed_shape(ShapeFill::Spam, 0.0625), 1);
    assert_eq!(notes.len(), 20, "五行 × 每行四个门（gate=120 tick）");
    assert!(
        notes.iter().all(|n| (n.length - gate).abs() < 1e-3),
        "Auto + Round 应铺满整门"
    );
}

#[test]
fn test_outline_spam_chops_outline_without_filling_inside() {
    let editor = editor_with_track(1);
    let gate = 0.0625 * f32::from(editor.editor_state.view.ppq);
    let notes = editor.shape_notes_of(&closed_shape(ShapeFill::OutlineSpam, 0.0625), 1);
    assert!(!notes.is_empty(), "轮廓仍应生成音符");
    assert!(
        notes.iter().all(|n| n.length <= gate + 1e-3),
        "描边应被门长切开，不再保留整行 480 tick 长音"
    );
}

#[test]
fn test_empty_shape_stays_outline_only() {
    let editor = editor_with_track(1);
    let notes = editor.shape_notes_of(&closed_shape(ShapeFill::Empty, 0.25), 1);
    assert!(!notes.is_empty());
    assert!(
        notes.iter().any(|n| n.length < 100.0),
        "Empty 应只有轮廓短音，不是四条整行长音"
    );
}

fn note_count(editor: &Editor, track: usize) -> usize {
    editor.editor_state.data.track_notes(track).len()
}

fn first_shape_id(editor: &Editor) -> u64 {
    editor.editor_state.shape_store.ids()[0]
}

#[test]
fn test_load_shapes_registers_without_generating_notes() {
    let mut editor = editor_with_track(1);
    editor.load_shapes(vec![straight_shape(1, 960.0), straight_shape(1, 480.0)]);

    assert_eq!(editor.editor_state.shape_store.len(), 2, "两个形状应被登记");
    assert_eq!(note_count(&editor, 1), 0, "加载只登记，不得凭空生成音符");
    assert_eq!(
        editor.shapes_snapshot().len(),
        2,
        "可持久化快照应含两个形状"
    );
}

#[test]
fn test_create_shape_generates_notes_and_registers() {
    let mut editor = editor_with_track(1);
    assert!(editor.create_shape(straight_shape(1, 960.0)));

    let id = first_shape_id(&editor);
    let count = note_count(&editor, 1);
    assert!(count > 0, "斜线应生成音符（跨越多个音高行）");
    assert_eq!(
        editor.editor_state.shape_store.notes_of(id).len(),
        count,
        "形状的音符指纹应与文档实际音符数一致"
    );
}

#[test]
fn test_create_shape_rejects_conductor_track() {
    let mut editor = editor_with_track(1);
    assert!(
        !editor.create_shape(straight_shape(0, 960.0)),
        "Conductor 轨应被拒绝"
    );
    assert!(editor.editor_state.shape_store.is_empty());
    assert_eq!(note_count(&editor, 0), 0);
}

#[test]
fn test_edit_shape_replaces_notes_without_orphans() {
    let mut editor = editor_with_track(1);
    assert!(editor.create_shape(straight_shape(1, 480.0)));
    let id = first_shape_id(&editor);
    let before = note_count(&editor, 1);

    // 跨更多音高行（横向拉长不增加音符数：每行本来就一条）
    let taller = straight_shape(1, 1920.0)
        .with_geometry(ShapeGeometry::from_points([(0.0, 60.0), (960.0, 84.0)]));
    assert!(editor.edit_shape(id, taller));
    let after = note_count(&editor, 1);
    assert!(
        after > before,
        "跨更多音高行后音符应变多（{before} → {after}）"
    );
    assert_eq!(
        editor.editor_state.shape_store.notes_of(id).len(),
        after,
        "旧音符必须被按值清理干净（不得残留孤儿）"
    );
}

/// **S0-3 验收标准**：一次参数提交 = 一次撤销单元，形状参数与音符同时回退/前进
#[test]
fn test_edit_then_undo_restores_shape_params_and_notes() {
    let mut editor = editor_with_track(1);
    let mut model = straight_shape(1, 480.0);
    assert!(editor.create_shape(model.clone()));
    let id = first_shape_id(&editor);
    let created_notes = note_count(&editor, 1);
    let created_gate = editor
        .editor_state
        .shape_store
        .model(id)
        .expect("形状应存在")
        .style
        .gate_beats;

    // 改参数：几何跨更多音高行（音符数随之变化）+ 门长变化
    model.geometry = ShapeGeometry::from_points([(0.0, 60.0), (1920.0, 84.0)]);
    model.style.gate_beats = 0.5;
    assert!(editor.edit_shape(id, model.clone()));
    let edited_notes = note_count(&editor, 1);
    assert_eq!(
        editor
            .editor_state
            .shape_store
            .model(id)
            .expect("形状应存在")
            .style
            .gate_beats,
        0.5
    );
    assert_ne!(edited_notes, created_notes, "本次编辑应改变音符数");

    // 撤销：形状参数与音符同时回退
    assert!(editor.undo(), "撤销应成功");
    assert_eq!(
        editor
            .editor_state
            .shape_store
            .model(id)
            .expect("形状应存在")
            .style
            .gate_beats,
        created_gate,
        "形状参数应随撤销回退"
    );
    assert_eq!(note_count(&editor, 1), created_notes, "音符应随撤销回退");

    // 重做：两者同时前进
    assert!(editor.redo(), "重做应成功");
    assert_eq!(
        editor
            .editor_state
            .shape_store
            .model(id)
            .expect("形状应存在")
            .style
            .gate_beats,
        0.5,
        "形状参数应随重做前进"
    );
    assert_eq!(note_count(&editor, 1), edited_notes, "音符应随重做前进");
}

#[test]
fn test_delete_shape_then_undo_restores() {
    let mut editor = editor_with_track(1);
    assert!(editor.create_shape(straight_shape(1, 960.0)));
    let id = first_shape_id(&editor);
    let count = note_count(&editor, 1);

    assert!(editor.delete_shape(id));
    assert_eq!(note_count(&editor, 1), 0, "删除形状应清理它的音符");
    assert!(editor.shapes_snapshot().is_empty(), "已删除的形状不写盘");

    assert!(editor.undo(), "撤销删除应成功");
    assert!(
        editor
            .editor_state
            .shape_store
            .entry(id)
            .expect("形状应存在")
            .is_visible(),
        "撤销删除应恢复形状"
    );
    assert_eq!(note_count(&editor, 1), count, "撤销删除应恢复音符");
}

#[test]
fn test_edit_missing_shape_returns_false() {
    let mut editor = editor_with_track(1);
    assert!(!editor.edit_shape(999, straight_shape(1, 480.0)));
    assert!(!editor.delete_shape(999));
}

// ── S0-5：工程往返（snapshot → load）与 k 保真 ─────────────────────────

#[test]
fn test_shapes_snapshot_roundtrips_through_load() {
    let mut source = editor_with_track(1);
    assert!(source.create_shape(straight_shape(1, 480.0)));
    assert!(source.create_shape(straight_shape(1, 960.0)));
    let snapshot = source.shapes_snapshot();
    assert_eq!(snapshot.len(), 2);

    // 模拟「打开工程」：新编辑器先装载文档、再灌入形状
    let mut target = editor_with_track(1);
    target.load_shapes(snapshot.clone());

    assert_eq!(
        target.shapes_snapshot(),
        snapshot,
        "形状快照经 load_shapes 应原样还原（含 k / 几何 / 样式）"
    );
    assert_eq!(
        note_count(&target, 1),
        0,
        "load_shapes 只登记形状，音符由工程文档自身承载"
    );
}

#[test]
fn test_k_is_preserved_across_roundtrip() {
    let mut source = editor_with_track(1);
    let mut model = straight_shape(1, 480.0);
    model.k = 0.375; // 非默认视图比例：丢失即说明 k 未随形状持久化
    assert!(source.create_shape(model));
    let snapshot = source.shapes_snapshot();
    let id = first_shape_id(&source);

    let mut target = editor_with_track(1);
    target.load_shapes(snapshot);
    assert_eq!(
        target
            .editor_state
            .shape_store
            .model(id)
            .expect("形状应存在")
            .k,
        0.375,
        "视图比例 k 必须随形状还原（否则跨缩放会走形）"
    );
}

/// 再生成只看几何，**不随视口缩放变化**（厚度 1 时无屏幕空间加粗）
#[test]
fn test_regeneration_is_zoom_independent() {
    let mut editor = editor_with_track(1);
    let model = straight_shape(1, 960.0);
    assert!(editor.create_shape(model.clone()));
    let id = first_shape_id(&editor);
    let before = editor.editor_state.shape_store.notes_of(id).to_vec();

    // 改变视口缩放后，用**同一参数**再生成
    editor.editor_state.view.zoom_x *= 2.5;
    editor.editor_state.view.zoom_y *= 0.5;
    assert!(editor.edit_shape(id, model));

    let after = editor.editor_state.shape_store.notes_of(id).to_vec();
    assert_eq!(
        before.len(),
        after.len(),
        "音符数量不得随视口缩放变化（{} → {}）",
        before.len(),
        after.len()
    );
    for (a, b) in before.iter().zip(after.iter()) {
        assert_eq!(
            (a.tick, a.key, a.length),
            (b.tick, b.key, b.length),
            "音符几何不得随视口缩放变化"
        );
    }
}

/// DRAW-007：Glue 的 Boxes 坐标系必须覆盖形状包围盒（key 上下各扩 0.5）
#[test]
fn test_shape_box_of_expands_key_range() {
    let box_ = super::shape_box_of(&[(0.0, 60.0), (480.0, 72.0)]).expect("非空路径应有包围盒");
    assert_eq!(box_.tick_lo, 0.0);
    assert_eq!(box_.tick_hi, 480.0);
    assert_eq!(box_.key_lo, 59.5);
    assert_eq!(box_.key_hi, 72.5);
    assert!(super::shape_box_of(&[]).is_none());
}

/// DRAW-003：Tumours 是 Geometry 阶段算子，必须进入现有音符生成链路
#[test]
fn test_regeneration_applies_tumours_geometry_op() {
    let editor = editor_with_track(1);
    let mut model = straight_shape(1, 480.0);
    let base = editor.shape_notes_of(&model, 1);
    assert!(!base.is_empty(), "无算子的直线形状应生成音符");

    model.push_op(ShapeOp::Tumours(TumourParams {
        k: 0.25,
        dist: 1.0,
        length: 0.5,
        size: 4.0,
        ..TumourParams::default()
    }));
    let with_op = editor.shape_notes_of(&model, 1);
    assert!(!with_op.is_empty(), "Tumours 形状应生成音符");
    assert!(
        with_op.len() > base.len(),
        "凸起轮廓应增加音符（base={}, with_op={}）",
        base.len(),
        with_op.len()
    );
}
