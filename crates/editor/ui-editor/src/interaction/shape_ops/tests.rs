//! 形状提交与撤销测试（S0-3b 验收：改参数 → undo → 形状与音符**同时**回退）

use super::helpers::shape_box_of;
use crate::Editor;
use crate::tests::test_helpers::seed_notes;
use lumino_core::shape::{
    AreaSpot, CycleBy, CycleParams, FunnelParams, GateRangeParams, NestedParams, PatternParams,
    PolyStyle, PolygonParams, RangeDir, ShapeFill, ShapeGeometry, ShapeKind, ShapeModel, ShapeOp,
    ShapeRect, ShapeStyle, TextLayoutMode, TextOp, TextOpV2, TextParams, TileBy, TilesParams,
    TumourParams,
};
use lumino_editor_state::{DrawnShapeSource, PendingShapeRef};

mod drawing;

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

/// 多个 `TextV2` 时只用首个：钉住删除重复分派后选择顺序不变。
#[test]
fn test_text_v2_dispatch_uses_first_op() {
    let editor = Editor::new();
    let k = 20.0 / (480.0 * 0.1);
    let mut model = ShapeModel::new(0, 1, ShapeKind::Custom, k);
    model.geometry = ShapeGeometry::from_rect(ShapeRect::normalized(0.0, 60.0, 3840.0, 84.0));
    let first = TextOpV2 {
        text: "A".to_string(),
        params: TextParams::default(),
        layout: TextLayoutMode::Parametric,
        snap: 1920.0,
        merged: false,
        vertical: false,
        k,
    };
    let second = TextOpV2 {
        text: "B".to_string(),
        ..first.clone()
    };
    model.ops.push(ShapeOp::TextV2(first.clone()));
    model.ops.push(ShapeOp::TextV2(second.clone()));

    let via_dispatch = editor.shape_notes_of(&model, 1);
    if via_dispatch.is_empty() {
        eprintln!("跳过 test_text_v2_dispatch_uses_first_op：无可用系统字体");
        return;
    }
    let expected = editor.text_shape_notes_v2(&model, &first, 1);
    let other = editor.text_shape_notes_v2(&model, &second, 1);
    assert_eq!(via_dispatch, expected, "多个 TextV2 必须分派到首个");
    assert_ne!(expected, other, "不同文本应产生不同音符，保证测试有区分度");
}

#[test]
fn test_text_shape_notes_follow_geometry_translation() {
    let editor = editor_with_track(1);
    let mut model = ShapeModel::new(0, 1, ShapeKind::Custom, 1.0);
    model.geometry = ShapeGeometry::from_rect(ShapeRect::normalized(0.0, 60.0, 960.0, 64.0));
    model.ops.push(ShapeOp::Text(TextOp {
        text: "A".to_string(),
        params: TextParams {
            font: "Arial".to_string(),
            ..TextParams::default()
        },
        layout: TextLayoutMode::Parametric,
        snap: 480.0,
        merged: false,
        vertical: false,
    }));
    let base = editor.shape_notes_of(&model, 1);
    if base.is_empty() {
        eprintln!("跳过 test_text_shape_notes_follow_geometry_translation：无可用系统字体");
        return;
    }

    model.geometry.translate(480.0, 2.0);
    let shifted = editor.shape_notes_of(&model, 1);
    assert_eq!(base.len(), shifted.len(), "平移不应改变文字音符数量");
    for (a, b) in base.iter().zip(&shifted) {
        assert!(
            (b.tick - a.tick - 480.0).abs() < 1e-3,
            "文字 bbox 平移后 tick 应整体 +480：{} -> {}",
            a.tick,
            b.tick
        );
        assert_eq!(b.key, a.key + 2, "文字 bbox 平移后 key 应整体 +2");
    }
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

fn area_shape(colour: u8) -> ShapeModel {
    let mut model = closed_shape(ShapeFill::Fill, 0.25);
    model.style.areas = vec![AreaSpot {
        u: 0.5,
        v: 0.5,
        colour,
    }];
    model
}

#[test]
fn test_areas_colour_only_the_painted_region() {
    let editor = editor_with_track(1);
    let notes = editor.shape_notes_of(&area_shape(3), 1);
    assert!(!notes.is_empty(), "着 colour=3 应仍产生内部音符");
    assert!(
        notes.iter().all(|note| note.channel == 3),
        "所有内部音符应使用区域色 3，实际 {:?}",
        notes
            .iter()
            .map(|note| (note.key, note.tick, note.channel))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_area_colour_zero_suppresses_fill() {
    let editor = editor_with_track(1);
    let notes = editor.shape_notes_of(&area_shape(0), 1);
    assert!(notes.is_empty(), "colour=0 的区域不应产音符");
}

#[test]
fn test_areas_override_cycle_channels() {
    let editor = editor_with_track(1);
    let mut model = area_shape(3);
    model.ops.push(ShapeOp::Cycle(CycleParams {
        by: CycleBy::Step,
        n: 3,
        ..CycleParams::default()
    }));
    let notes = editor.shape_notes_of(&model, 1);
    assert!(!notes.is_empty());
    assert!(
        notes.iter().all(|note| note.channel == 3),
        "区域色必须优先于 Cycle，实际 {:?}",
        notes.iter().map(|note| note.channel).collect::<Vec<_>>()
    );
}

#[test]
fn test_areas_do_not_change_outline_only_shapes() {
    let editor = editor_with_track(1);
    let base = editor.shape_notes_of(&closed_shape(ShapeFill::Empty, 0.25), 1);
    let mut model = closed_shape(ShapeFill::Empty, 0.25);
    model.style.areas = vec![AreaSpot {
        u: 0.5,
        v: 0.5,
        colour: 0,
    }];
    let notes = editor.shape_notes_of(&model, 1);
    assert_eq!(notes, base, "Areas 只作用于内部填充，不应影响描边");
}

#[test]
fn test_areas_follow_geometry_scale() {
    let editor = editor_with_track(1);
    let mut scaled = area_shape(5);
    // 以原点为中心放大 2 倍；区域点仍是同一相对位置。
    for anchor in &mut scaled.geometry.anchors {
        anchor.pos.0 *= 2.0;
        anchor.pos.1 = 60.0 + (anchor.pos.1 - 60.0) * 2.0;
    }
    let notes = editor.shape_notes_of(&scaled, 1);
    assert!(!notes.is_empty());
    assert!(
        notes.iter().all(|note| note.channel == 5),
        "缩放后区域相对位置不变，实际 {:?}",
        notes
            .iter()
            .map(|note| (note.key, note.tick, note.channel))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_area_spot_outside_shape_falls_back_to_existing_fill() {
    let editor = editor_with_track(1);
    let base = editor.shape_notes_of(&closed_shape(ShapeFill::Fill, 0.25), 1);
    let mut model = closed_shape(ShapeFill::Fill, 0.25);
    model.style.areas = vec![AreaSpot {
        u: 1.5,
        v: 1.5,
        colour: 3,
    }];
    let notes = editor.shape_notes_of(&model, 1);
    assert_eq!(notes, base, "无效区域点不应改变既有填充");
}

#[test]
fn test_areas_follow_geometry_translation() {
    let editor = editor_with_track(1);
    let base = editor.shape_notes_of(&area_shape(5), 1);
    let mut moved = area_shape(5);
    moved.geometry.translate(480.0, 2.0);
    let shifted = editor.shape_notes_of(&moved, 1);
    assert_eq!(base.len(), shifted.len());
    for (a, b) in base.iter().zip(&shifted) {
        assert_eq!(a.channel, b.channel, "平移不应改变区域色");
        assert!((b.tick - a.tick - 480.0).abs() < 1e-3);
        assert_eq!(b.key, a.key + 2);
    }
}

#[test]
fn test_use10_allows_colour_16_as_channel_10() {
    let mut editor = editor_with_track(1);
    let model = area_shape(16);

    // 未启用通道 10：colour=16 被跳过，区域点不生效（退回普通填充，channel 0）
    let off = editor.shape_notes_of(&model, 1);
    assert!(
        off.iter().all(|note| note.channel == 0),
        "use10=false 时 colour=16 不应生效"
    );

    editor.editor_state.use10 = true;
    let on = editor.shape_notes_of(&model, 1);
    assert!(!on.is_empty(), "use10=true 时 colour=16 应可生成音符");
    assert!(
        on.iter().all(|note| note.channel == 9),
        "colour=16 应映射到 GM 通道 10（内部索引 9），实际 {:?}",
        on.iter().map(|note| note.channel).collect::<Vec<_>>()
    );
}

#[test]
fn test_borders_without_colour_boundary_keeps_fill_channel() {
    let editor = editor_with_track(1);
    let mut model = area_shape(3);
    model.style.borders = true;
    model.style.colour_slot = 7;
    let notes = editor.shape_notes_of(&model, 1);
    assert!(!notes.is_empty());
    assert!(
        notes.iter().all(|note| note.channel == 3),
        "单色区域没有颜色交界，borders 不得把填充改成描边通道"
    );
}

#[test]
fn test_apart_sends_outline_to_colour_slot() {
    let editor = editor_with_track(1);
    let mut model = closed_shape(ShapeFill::Empty, 0.25);
    model.style.colour_slot = 3;

    let plain = editor.shape_notes_of(&model, 1);
    assert!(!plain.is_empty(), "描边图应产生音符");
    assert!(
        plain.iter().all(|note| note.channel == 0),
        "apart=false 时描边仍是通道 0"
    );

    model.style.apart = true;
    let apart = editor.shape_notes_of(&model, 1);
    assert!(!apart.is_empty());
    assert!(
        apart.iter().all(|note| note.channel == 3),
        "apart=true 时描边应走 colour_slot，实际 {:?}",
        apart.iter().map(|note| note.channel).collect::<Vec<_>>()
    );
}

#[test]
fn test_apart_does_not_change_interior() {
    let editor = editor_with_track(1);
    let mut model = closed_shape(ShapeFill::Fill, 0.25);
    model.style.colour_slot = 3;

    let before = editor.shape_notes_of(&model, 1);
    model.style.apart = true;
    let after = editor.shape_notes_of(&model, 1);

    assert_eq!(before.len(), after.len(), "apart 不应增删内部音符");
    assert!(
        after.iter().all(|note| note.channel == 0),
        "内部通道不受 apart 影响"
    );
}

fn arc_shape(mid_key: f32) -> ShapeModel {
    let mut model = ShapeModel::new(0, 1, ShapeKind::Arc, 1.0);
    model.geometry = ShapeGeometry::from_points([(0.0, 60.0), (240.0, mid_key), (480.0, 60.0)]);
    model
}

fn interior_shape(op: ShapeOp) -> ShapeModel {
    let mut model = closed_shape(ShapeFill::Fill, 0.25);
    model.ops.push(op);
    model
}

#[test]
fn test_nested_fill_emits_layers_and_channels() {
    let editor = editor_with_track(1);
    let model = interior_shape(ShapeOp::Nested(NestedParams {
        steps: 3,
        last: 0.5,
        colours: 3,
        ..NestedParams::default()
    }));
    let notes = editor.shape_notes_of(&model, 1);
    assert!(!notes.is_empty());
    let channels = notes
        .iter()
        .map(|n| n.channel)
        .collect::<std::collections::BTreeSet<_>>();
    assert!(channels.contains(&0) && channels.contains(&1) && channels.contains(&2));
    assert!(notes.iter().all(|n| n.channel < 16));
}

#[test]
fn test_tiles_fill_emits_coloured_blocks() {
    let editor = editor_with_track(1);
    let model = interior_shape(ShapeOp::Tiles(TilesParams {
        length: 120.0,
        keys: 2.0,
        colours: 2,
        by: TileBy::Cols,
        ..TilesParams::default()
    }));
    let notes = editor.shape_notes_of(&model, 1);
    assert!(!notes.is_empty(), "Tiles 应生成内部块");
    assert!(notes.iter().any(|n| n.channel == 1), "多列应轮转通道");
    assert!(notes.iter().all(|n| n.channel < 16));
}

fn funnel_params(fill: ShapeFill, reverse: bool) -> FunnelParams {
    FunnelParams {
        lines: vec![[[0.0, 20.0], [100.0, 20.0]]],
        wall: [[100.0, 10.0], [100.0, 30.0]],
        starts: Vec::new(),
        fill,
        reverse,
    }
}

fn funnel_shape(fill: ShapeFill, reverse: bool) -> ShapeModel {
    let mut model = ShapeModel::new(0, 1, ShapeKind::Funnel, 1.0);
    model.geometry =
        ShapeGeometry::from_points([(0.0, 20.0), (100.0, 20.0), (100.0, 10.0), (100.0, 30.0)]);
    model
        .ops
        .push(ShapeOp::Funnel(funnel_params(fill, reverse)));
    model
}

#[test]
fn test_funnel_spam_notes_are_inside_slant_boxes() {
    let editor = editor_with_track(1);
    let notes = editor.shape_notes_of(&funnel_shape(ShapeFill::Spam, false), 1);
    assert!(!notes.is_empty());
    let boxes =
        lumino_editor_state::shape_ops::funnel_slant_boxes(&funnel_params(ShapeFill::Spam, false));
    for note in &notes {
        let point = (f64::from(note.tick), f64::from(note.key));
        assert!(
            boxes.iter().any(|box_| {
                box_.inverse(point).is_some_and(|(u, f)| {
                    (-1e-4..=1.0 + 1e-4).contains(&u) && (-1e-4..=1.0 + 1e-4).contains(&f)
                })
            }),
            "音符 {note:?} 应落在漏斗内"
        );
    }
}

#[test]
fn test_funnel_empty_outline_differs_from_spam() {
    let editor = editor_with_track(1);
    let empty = editor.shape_notes_of(&funnel_shape(ShapeFill::Empty, false), 1);
    let spam = editor.shape_notes_of(&funnel_shape(ShapeFill::Spam, false), 1);
    assert!(!empty.is_empty());
    assert!(!spam.is_empty());
    assert_ne!(
        empty.len(),
        spam.len(),
        "Empty 描边与 Spam 填充音符数应不同"
    );
}

#[test]
fn test_funnel_reverse_order() {
    let editor = editor_with_track(1);
    let notes = editor.shape_notes_of(&funnel_shape(ShapeFill::Spam, true), 1);
    assert!(notes.len() > 1);
    let first = notes.first().expect("应有音符").tick;
    let last = notes.last().expect("应有音符").tick;
    assert!(first >= last, "反向漏斗应按时间倒序输出");
}

fn pattern_shape() -> ShapeModel {
    let mut model = ShapeModel::new(0, 1, ShapeKind::Line, 1.0);
    model.geometry = ShapeGeometry::from_points([(0.0, 60.0), (480.0, 60.0)]);
    model.ops.push(ShapeOp::Pattern(PatternParams {
        formula: "sin(2*pi*x)".into(),
        vars: Default::default(),
        loops: 1.0,
        k: 1.0,
        ..PatternParams::default()
    }));
    model
}

#[test]
fn test_pattern_generates_sideways_wave() {
    let editor = editor_with_track(1);
    let notes = editor.shape_notes_of(&pattern_shape(), 1);
    assert!(!notes.is_empty(), "Pattern 应生成描边音符");
    let hi = notes.iter().map(|n| n.key).max().unwrap_or(0);
    assert!(hi > 60, "正弦应向上偏移，实际最高 key {hi}");
}

fn polygon_shape(style: PolyStyle, points: u16) -> ShapeModel {
    let rect = lumino_core::shape::ShapeRect {
        tick_lo: 100.0,
        key_lo: 60.0,
        tick_hi: 500.0,
        key_hi: 68.0,
    };
    let mut model = ShapeModel::new(0, 1, ShapeKind::Custom, 1.0);
    model.geometry = ShapeGeometry::from_rect(rect);
    model.ops.push(ShapeOp::Polygon(PolygonParams {
        points,
        style,
        ..PolygonParams::default()
    }));
    model
}

#[test]
fn test_polygon_generates_outline_inside_box() {
    let editor = editor_with_track(1);
    let notes = editor.shape_notes_of(&polygon_shape(PolyStyle::Polygon, 4), 1);
    assert!(!notes.is_empty(), "方形应生成描边音符");
    assert!(notes.iter().all(|n| (100.0..=500.0).contains(&n.tick)));
    assert!(notes.iter().all(|n| (60..=68).contains(&n.key)));
}

#[test]
fn test_cross_6_2_generates_two_loops() {
    let editor = editor_with_track(1);
    let notes = editor.shape_notes_of(&polygon_shape(PolyStyle::Cross, 6), 1);
    assert!(!notes.is_empty(), "6/2 两个三角形环都应产出音符");
}

#[test]
fn test_arc_shape_uses_three_point_circle() {
    let editor = editor_with_track(1);
    let notes = editor.shape_notes_of(&arc_shape(66.0), 1);
    assert!(!notes.is_empty(), "Arc 应生成描边音符");
    let hi = notes.iter().map(|n| n.key).max().unwrap_or(0);
    assert!(hi >= 66, "弧应经过中间点 key=66，实际最高 {hi}");
}

/// DRAW-014：生成结果与「先按屏幕空间求弧再映射」的参考实现逐音符一致。
///
/// 参考实现独立走一遍 `(tick,key) → (beats,key) → arc_points_scaled → tick`
/// 的映射，再喂给同一条描边链路；若 `shape_notes_of` 误把弧当成折线
/// （例如直接 `flatten_path(anchors)`），逐音符比对会立刻失败。
#[test]
fn test_arc_notes_match_screen_space_reference() {
    let editor = editor_with_track(1);
    let model = arc_shape(66.0);
    let notes = editor.shape_notes_of(&model, 1);

    let ppq = f64::from(editor.editor_state.view.ppq);
    let to_beat = |tick: f32, key: f32| (f64::from(tick) / ppq, f64::from(key));
    let reference: Vec<(f64, f64)> = lumino_editor_state::shape_ops::arc_points_scaled(
        to_beat(0.0, 60.0),
        to_beat(240.0, 66.0),
        to_beat(480.0, 60.0),
        lumino_editor_state::shape_ops::arc_k_or_one(f64::from(model.k)),
    )
    .into_iter()
    .map(|(beat, key)| (beat * ppq, key))
    .collect();

    let key_count = editor.editor_state.view.key_count as i32;
    let min_ticks = crate::interaction::line_tool::paths::min_note_length_ticks(
        editor.editor_state.line_tool.min_note_division,
        editor.editor_state.view.ppq,
    );
    let expected = editor.shape_outline_notes(&reference, 1, key_count, min_ticks, false, &model);
    assert_eq!(notes, expected, "Arc 生成必须与屏幕空间参考逐音符一致");
}

#[test]
fn test_collinear_arc_falls_back_to_polyline() {
    let editor = editor_with_track(1);
    let notes = editor.shape_notes_of(&arc_shape(60.0), 1);
    assert!(!notes.is_empty(), "退化弧应回退为折线而不是无产物");
    assert!(notes.iter().all(|n| n.key == 60), "共线三点应保持直线");
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
fn test_spam_gate_range_varies_gate_across_time() {
    let editor = editor_with_track(1);
    let mut model = closed_shape(ShapeFill::Spam, 0.0625);
    model.push_op(ShapeOp::GateRange(GateRangeParams {
        to: 0.25,
        dir: RangeDir::Time,
        ..GateRangeParams::default()
    }));
    let notes = editor.shape_notes_of(&model, 1);
    assert!(!notes.is_empty());
    let min = notes.iter().map(|n| n.length).fold(f32::INFINITY, f32::min);
    let max = notes
        .iter()
        .map(|n| n.length)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(max > min + 1.0, "Time Range 的门长应沿时间增大");
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

mod part2;
