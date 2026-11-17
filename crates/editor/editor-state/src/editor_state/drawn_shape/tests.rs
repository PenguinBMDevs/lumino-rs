use super::*;

fn rect_source() -> DrawnShapeSource {
    DrawnShapeSource::Shape {
        kind: ShapeKind::Rectangle,
        rect: (0.0, 60.0, 4.0, 64.0),
        shift_constrained: false,
        filled: false,
        apex_high: true,
    }
}

fn note(tick: f32, key: u16) -> ShapeNote {
    ShapeNote {
        track: 1,
        tick,
        key,
        length: 1.0,
        velocity: 100,
        channel: 0,
    }
}

#[test]
fn test_add_assigns_stable_ids_and_notes() {
    let mut st = ShapeSelectState::default();
    let a = st.add(1, Some(7), rect_source(), vec![note(0.0, 60)]);
    let b = st.add(1, None, rect_source(), vec![]);
    assert_ne!(a, b, "不同图形必须得到不同 ID");
    assert_eq!(st.len(), 2);
    assert_eq!(st.shapes()[0].notes.len(), 1);
}

#[test]
fn test_select_only_validates_existence_and_visibility() {
    let mut st = ShapeSelectState::default();
    let a = st.add(1, Some(10), rect_source(), vec![]);
    st.select_only(a);
    assert_eq!(st.selected_ids(), &[a]);
    st.select_only(9999);
    assert!(st.selected_ids().is_empty(), "不存在的 ID 应清空选中集");
    // 撤销创建后不可再选中
    st.select_only(a);
    st.on_undo_group(10);
    assert!(st.selected_ids().is_empty(), "被隐藏的图形不应保持选中");
}

#[test]
fn test_selection_multi_add_remove_toggle() {
    let mut st = ShapeSelectState::default();
    let a = st.add(1, None, rect_source(), vec![]);
    let b = st.add(1, None, rect_source(), vec![]);
    let c = st.add(1, None, rect_source(), vec![]);

    assert!(st.add_to_selection(a));
    assert!(st.add_to_selection(b));
    assert!(!st.add_to_selection(b), "重复加入应无变化");
    assert_eq!(st.selected_ids(), &[a, b], "保持加入顺序");
    assert_eq!(st.selection_len(), 2);
    assert!(st.is_selected(a) && !st.is_selected(c));

    // 切换：c 加入（true）、a 移出（false）
    assert!(st.toggle_selection(c));
    assert!(!st.toggle_selection(a));
    assert_eq!(st.selected_ids(), &[b, c]);

    // 集合替换：去重 + 过滤不存在 / 不可见
    st.select_all_of([a, c, c, 9999]);
    assert_eq!(st.selected_ids(), &[a, c]);

    st.clear_selection();
    assert!(st.selected_ids().is_empty());
    assert!(!st.add_to_selection(9999), "不存在的 ID 不可加入");
}

#[test]
fn test_selection_bounds_on_is_union_over_track() {
    let mut st = ShapeSelectState::default();
    let a = st.add(1, None, rect_source(), vec![]); // 0..4 × 60..64
    let b = st.add(
        1,
        None,
        DrawnShapeSource::Shape {
            kind: ShapeKind::Rectangle,
            rect: (10.0, 50.0, 20.0, 55.0),
            shift_constrained: false,
            filled: false,
            apex_high: true,
        },
        vec![],
    );
    let c = st.add(2, None, rect_source(), vec![]);
    st.select_all_of([a, b, c]);

    assert_eq!(
        st.selection_bounds_on(1),
        Some((0.0, 20.0, 50.0, 64.0)),
        "多选外接框应为该轨全部选中图形的并集"
    );
    assert_eq!(st.selection_bounds_on(2), Some((0.0, 4.0, 60.0, 64.0)));
    assert_eq!(
        st.selection_bounds_on(3),
        None,
        "无选中图形的音轨应无外接框"
    );

    st.select_only(a);
    assert_eq!(st.selection_bounds_on(1), Some((0.0, 4.0, 60.0, 64.0)));

    // 空折线几何不参与并集
    let e = st.add(
        1,
        None,
        DrawnShapeSource::Polyline { points: Vec::new() },
        vec![],
    );
    st.select_all_of([e]);
    assert_eq!(st.selection_bounds_on(1), None);
}

#[test]
fn test_selection_bounds_normalizes_reversed_rect() {
    let mut st = ShapeSelectState::default();
    let s = DrawnShapeSource::Shape {
        kind: ShapeKind::Rectangle,
        rect: (9.0, 70.0, 1.0, 62.0),
        shift_constrained: false,
        filled: false,
        apex_high: true,
    };
    let a = st.add(1, None, s, vec![]);
    st.select_only(a);
    assert_eq!(st.selection_bounds_on(1), Some((1.0, 9.0, 62.0, 70.0)));
}

#[test]
fn test_selected_shapes_on_filters_track_and_visibility() {
    let mut st = ShapeSelectState::default();
    let a = st.add(1, Some(5), rect_source(), vec![]);
    let b = st.add(2, None, rect_source(), vec![]);
    st.select_all_of([a, b]);
    assert_eq!(st.selected_shapes().count(), 2);
    assert_eq!(st.selected_shapes_on(1).count(), 1);
    assert_eq!(st.selected_shapes_on(2).count(), 1);
    // 撤销创建 → a 隐藏 → 自动收敛出选中集
    st.on_undo_group(5);
    assert_eq!(st.selected_shapes_on(1).count(), 0);
    assert_eq!(st.selected_ids(), &[b], "隐藏者应被收敛、其余保留");
}

#[test]
fn test_visible_on_filters_track_and_state() {
    let mut st = ShapeSelectState::default();
    st.add(1, Some(10), rect_source(), vec![]);
    st.add(2, Some(10), rect_source(), vec![]);
    st.add(1, Some(20), rect_source(), vec![]);
    assert_eq!(st.visible_on(1).count(), 2);
    st.on_undo_group(10);
    assert_eq!(st.visible_on(1).count(), 1);
    assert_eq!(st.visible_on(2).count(), 0);
    st.on_redo_group(10);
    assert_eq!(st.visible_on(1).count(), 2);
}

#[test]
fn test_delete_and_reconcile_restores() {
    let mut st = ShapeSelectState::default();
    let a = st.add(1, Some(1), rect_source(), vec![note(0.0, 60)]);
    st.select_only(a);
    assert!(st.mark_deleted(a));
    assert_eq!(st.visible_on(1).count(), 0, "删除后不可见");
    assert!(st.selected_ids().is_empty(), "删除应收敛选中集");
    // 撤销删除 → 音符回来了 → 对账恢复可见
    st.reconcile_alive(&[(a, true)]);
    assert_eq!(st.visible_on(1).count(), 1, "撤销删除应恢复");
    // 重做删除 → 音符又没了
    st.reconcile_alive(&[(a, false)]);
    assert_eq!(st.visible_on(1).count(), 0, "重做删除应再次隐藏");
}

#[test]
fn test_reconcile_ignores_never_deleted_shapes() {
    let mut st = ShapeSelectState::default();
    let a = st.add(1, None, rect_source(), vec![note(0.0, 60)]);
    // 未走过删除流程的图形：即使音符不在，也不因对账被隐藏（避免误伤手动擦除场景）
    st.reconcile_alive(&[(a, false)]);
    assert_eq!(st.visible_on(1).count(), 1);
    // 空音符图形同样不参与对账
    let b = st.add(1, None, rect_source(), vec![]);
    st.mark_deleted(b);
    st.reconcile_alive(&[(b, false)]);
    assert_eq!(st.visible_on(1).count(), 1);
}

#[test]
fn test_translate_updates_geometry_and_notes_and_undoes() {
    let mut st = ShapeSelectState::default();
    let a = st.add(1, None, rect_source(), vec![note(10.0, 60)]);
    assert!(st.translate_shape(a, Some(5), 100.0, 2.0));
    let s = &st.shapes()[0];
    match &s.source {
        DrawnShapeSource::Shape { rect, .. } => assert_eq!(*rect, (100.0, 62.0, 104.0, 66.0)),
        other => panic!("期望 Shape，实际 {other:?}"),
    }
    assert_eq!(s.notes[0].tick, 110.0);
    assert_eq!(s.notes[0].key, 62);
    st.on_undo_group(5);
    let s = &st.shapes()[0];
    match &s.source {
        DrawnShapeSource::Shape { rect, .. } => assert_eq!(*rect, (0.0, 60.0, 4.0, 64.0)),
        other => panic!("期望 Shape，实际 {other:?}"),
    }
    assert_eq!(s.notes[0].tick, 10.0);
    assert_eq!(s.notes[0].key, 60);
    st.on_redo_group(5);
    assert_eq!(st.shapes()[0].notes[0].tick, 110.0);
}

#[test]
fn test_translate_multiple_shapes_share_one_group() {
    let mut st = ShapeSelectState::default();
    let a = st.add(1, None, rect_source(), vec![note(10.0, 60)]);
    let b = st.add(1, None, rect_source(), vec![note(20.0, 62)]);
    st.select_all_of([a, b]);
    assert!(st.translate_shape(a, Some(9), 5.0, 1.0));
    assert!(st.translate_shape(b, Some(9), 5.0, 1.0));
    // 同组一次撤销应把两个图形一起还原
    st.on_undo_group(9);
    assert_eq!(st.shapes()[0].notes[0].tick, 10.0);
    assert_eq!(st.shapes()[1].notes[0].tick, 20.0);
    // 重做一起前进
    st.on_redo_group(9);
    assert_eq!(st.shapes()[0].notes[0].tick, 15.0);
    assert_eq!(st.shapes()[1].notes[0].tick, 25.0);
}

#[test]
fn test_translate_clamps_delta_without_shrinking() {
    let mut src = DrawnShapeSource::Shape {
        kind: ShapeKind::Rectangle,
        rect: (100.0, 60.0, 500.0, 64.0),
        shift_constrained: false,
        filled: false,
        apex_high: true,
    };
    let applied = src.translate(-200.0, 0.0);
    assert_eq!(applied, (-100.0, 0.0), "只能移动到 tick 0");
    assert_eq!(
        src.bounds(),
        Some((0.0, 400.0, 60.0, 64.0)),
        "宽度必须保持 400，不能被逐坐标压扁"
    );
}

#[test]
fn test_drag_clamps_total_delta_at_tick_zero() {
    let mut st = ShapeSelectState::default();
    let src = DrawnShapeSource::Shape {
        kind: ShapeKind::Rectangle,
        rect: (100.0, 60.0, 500.0, 64.0),
        shift_constrained: false,
        filled: false,
        apex_high: true,
    };
    let id = st.add(1, None, src, vec![]);
    st.select_only(id);
    st.begin_drag(0.0, 60.0);
    st.update_drag(-200.0, 60.0, 1.0);
    assert_eq!(
        st.drag().expect("拖拽态应存在").delta_tick,
        -100.0,
        "偏移必须按起点边界夹取"
    );
    assert_eq!(st.end_drag(), Some((-100.0, 0.0)));
}

#[test]
fn test_translate_shape_keeps_notes_in_sync_at_boundary() {
    let mut st = ShapeSelectState::default();
    let src = DrawnShapeSource::Shape {
        kind: ShapeKind::Rectangle,
        rect: (100.0, 60.0, 500.0, 64.0),
        shift_constrained: false,
        filled: false,
        apex_high: true,
    };
    let id = st.add(1, None, src, vec![note(100.0, 60)]);
    assert!(st.translate_shape(id, None, -200.0, 0.0));
    let shape = &st.shapes()[0];
    assert_eq!(shape.source.bounds(), Some((0.0, 400.0, 60.0, 64.0)));
    assert_eq!(
        shape.notes[0].tick, 0.0,
        "音符必须按实际应用的偏移平移，不能与几何脱节"
    );
}

#[test]
fn test_drag_lifecycle() {
    let mut st = ShapeSelectState::default();
    let a = st.add(1, None, rect_source(), vec![]);
    // 无选中 → 不进入拖拽态
    st.begin_drag(0.0, 60.0);
    assert!(!st.is_dragging(), "无选中不应起拖");
    st.select_only(a);
    st.begin_drag(0.0, 60.0);
    assert!(st.is_dragging());
    // snap = 10：raw 6 → 对齐到 10；raw 14 → 10
    st.update_drag(6.0, 62.4, 10.0);
    let d = st.drag().expect("拖拽态应存在");
    assert_eq!(d.delta_tick, 10.0);
    assert_eq!(d.delta_key, 2.0);
    st.update_drag(14.0, 62.4, 10.0);
    assert_eq!(st.drag().expect("拖拽态应存在").delta_tick, 10.0);
    assert_eq!(st.end_drag(), Some((10.0, 2.0)));
    assert!(!st.is_dragging());
}

#[test]
fn test_drag_without_movement_returns_none() {
    let mut st = ShapeSelectState::default();
    let a = st.add(1, None, rect_source(), vec![]);
    st.select_only(a);
    st.begin_drag(0.0, 60.0);
    st.update_drag(0.4, 60.0, 1.0);
    assert_eq!(st.end_drag(), None, "原地按一下不应产生移动历史");
}

#[test]
fn test_bounds_for_shape_and_polyline() {
    let s = DrawnShapeSource::Shape {
        kind: ShapeKind::Rectangle,
        rect: (4.0, 64.0, 0.0, 60.0),
        shift_constrained: false,
        filled: false,
        apex_high: true,
    };
    assert_eq!(s.bounds(), Some((0.0, 4.0, 60.0, 64.0)));
    let p = DrawnShapeSource::Polyline {
        points: vec![(5.0, 61.0), (1.0, 70.0)],
    };
    assert_eq!(p.bounds(), Some((1.0, 5.0, 61.0, 70.0)));
    assert_eq!(DrawnShapeSource::Polyline { points: vec![] }.bounds(), None);
}

#[test]
fn test_drag_and_marquee_are_mutually_exclusive() {
    // 同一时刻只能跑一个手势：否则 moved/released 的早退分支会互相错配
    let mut st = ShapeSelectState::default();
    let a = st.add(1, None, rect_source(), vec![]);
    st.select_only(a);
    st.begin_drag(0.0, 60.0);
    assert!(st.is_dragging());
    st.begin_marquee(0.0, 60.0, false);
    assert!(!st.is_dragging(), "起框应收敛拖拽态");
    assert!(st.is_marqueeing());
    st.begin_drag(0.0, 60.0);
    assert!(st.is_dragging(), "起拖应收敛拉框态");
    assert!(!st.is_marqueeing());
}

#[test]
fn test_marquee_lifecycle_normalizes_rect_and_keeps_additive() {
    let mut st = ShapeSelectState::default();
    assert!(!st.is_marqueeing());
    st.begin_marquee(10.0, 70.0, false);
    assert!(st.is_marqueeing());
    // 反向拖动（左下 → 右上）应被规范化成 (min, max) 布局
    st.update_marquee(2.0, 60.0);
    let m = st.marquee().expect("拉框态应存在");
    assert_eq!(m.rect(), (2.0, 10.0, 60.0, 70.0));
    assert!(!m.additive);
    let taken = st.end_marquee().expect("结束应返回矩形");
    assert_eq!(taken.rect(), (2.0, 10.0, 60.0, 70.0));
    assert!(!st.is_marqueeing(), "结束后拉框态应清空");
    assert!(st.end_marquee().is_none(), "重复结束应为 None");
    // 修饰键起框 → 标记为并入
    st.begin_marquee(0.0, 60.0, true);
    assert!(st.marquee().expect("拉框态应存在").additive);
    st.end_marquee();
}

#[test]
fn test_clear_resets_marquee() {
    let mut st = ShapeSelectState::default();
    st.add(1, None, rect_source(), vec![]);
    st.begin_marquee(0.0, 60.0, false);
    st.clear();
    assert!(!st.is_marqueeing());
    assert!(st.is_empty());
}

#[test]
fn test_clear_resets_everything() {
    let mut st = ShapeSelectState::default();
    let a = st.add(1, Some(3), rect_source(), vec![]);
    st.select_only(a);
    st.clear();
    assert!(st.is_empty());
    assert!(st.selected_ids().is_empty());
    assert_eq!(st.selection_len(), 0);
    assert!(!st.is_dragging());
    assert_eq!(st.add(1, None, rect_source(), vec![]), 0);
}

// ── 待确认产物镜像（未 √ 的几何进入选中域） ──────────────

#[test]
fn test_add_pending_mirror_has_no_notes_and_shares_track_domain() {
    let mut st = ShapeSelectState::default();
    let confirmed = st.add(1, None, rect_source(), vec![note(0.0, 60)]);
    let mirror = st.add_pending(1, PendingShapeRef::Shape(0), rect_source());

    let m = st
        .shapes()
        .iter()
        .find(|s| s.id == mirror)
        .expect("镜像应存在");
    assert!(m.notes.is_empty(), "镜像不得携带音符（音符只属于 √）");
    assert_eq!(m.group, None, "镜像不绑定历史分组");
    assert_eq!(m.pending, Some(PendingShapeRef::Shape(0)));
    assert_eq!(st.pending_of(mirror), Some(PendingShapeRef::Shape(0)));
    assert_eq!(st.pending_of(confirmed), None, "已确认图形无待确认来源");
    assert_eq!(st.pending_mirror(PendingShapeRef::Shape(0)), Some(mirror));

    // 镜像的几何尚未绑定音轨：任何非 Conductor 轨都可见可选中
    assert_eq!(st.visible_on(1).count(), 2);
    assert_eq!(st.visible_on(2).count(), 1, "镜像应随任意普通轨可见");
    assert_eq!(st.visible_on(3).count(), 1);
    assert_eq!(
        st.visible_on(0).count(),
        0,
        "Conductor 轨（track 0）不渲染待确认预览，镜像也不该出现"
    );
    // 已确认图形仍是严格的单轨归属
    let ids: Vec<u64> = st.visible_on(2).map(|s| s.id).collect();
    assert_eq!(ids, vec![mirror]);
}

#[test]
fn test_set_source_overwrites_mirror_geometry() {
    let mut st = ShapeSelectState::default();
    let mirror = st.add_pending(1, PendingShapeRef::CurvePath(0), rect_source());
    assert_eq!(st.selection_bounds_on(1), None, "未选中时无外接框");
    st.select_only(mirror);
    assert_eq!(st.selection_bounds_on(1), Some((0.0, 4.0, 60.0, 64.0)));

    let moved = DrawnShapeSource::Polyline {
        points: vec![(10.0, 62.0), (14.0, 66.0)],
    };
    assert!(st.set_source(mirror, moved.clone()));
    assert_eq!(
        st.shapes()[0].source,
        moved,
        "几何应被覆写为 owning 侧最新值"
    );
    assert_eq!(st.selection_bounds_on(1), Some((10.0, 14.0, 62.0, 66.0)));
    assert!(!st.set_source(9999, moved), "不存在的 ID 应返回 false");
}

#[test]
fn test_remove_pending_all_keeps_confirmed_shapes() {
    let mut st = ShapeSelectState::default();
    let a = st.add(1, None, rect_source(), vec![note(0.0, 60)]);
    let m1 = st.add_pending(1, PendingShapeRef::Shape(0), rect_source());
    let m2 = st.add_pending(1, PendingShapeRef::BrushStroke(0), rect_source());
    st.select_all_of([a, m1, m2]);
    assert_eq!(st.selection_len(), 3);

    st.remove_pending_all();
    assert_eq!(st.len(), 1, "只剩已确认图形");
    assert_eq!(st.shapes()[0].id, a);
    assert_eq!(st.selected_ids(), &[a], "镜像退场后选中集应自动收敛");
    assert!(st.pending_shapes().next().is_none());
}

#[test]
fn test_shift_pending_indices_only_affects_later_same_container() {
    let mut st = ShapeSelectState::default();
    let s0 = st.add_pending(1, PendingShapeRef::Shape(0), rect_source());
    let s1 = st.add_pending(1, PendingShapeRef::Shape(1), rect_source());
    let c0 = st.add_pending(1, PendingShapeRef::CurvePath(0), rect_source());
    let b2 = st.add_pending(1, PendingShapeRef::BrushStroke(2), rect_source());

    // 形状容器删掉下标 0：同容器的 1 → 0；其它容器不受影响
    st.shift_pending_indices(PendingShapeRef::Shape(0));
    assert_eq!(st.pending_of(s0), Some(PendingShapeRef::Shape(0)));
    assert_eq!(
        st.pending_of(s1),
        Some(PendingShapeRef::Shape(0)),
        "同容器后续下标应前移"
    );
    assert_eq!(
        st.pending_mirror(PendingShapeRef::Shape(1)),
        None,
        "前移后不应再有旧下标"
    );
    assert_eq!(st.pending_of(c0), Some(PendingShapeRef::CurvePath(0)));
    assert_eq!(st.pending_of(b2), Some(PendingShapeRef::BrushStroke(2)));
}

#[test]
fn test_pending_mirror_participates_in_selection_and_bounds_union() {
    let mut st = ShapeSelectState::default();
    let a = st.add(
        1,
        None,
        DrawnShapeSource::Shape {
            kind: ShapeKind::Rectangle,
            rect: (0.0, 60.0, 4.0, 64.0),
            shift_constrained: false,
            filled: false,
            apex_high: true,
        },
        vec![],
    );
    let m = st.add_pending(
        1,
        PendingShapeRef::Shape(0),
        DrawnShapeSource::Shape {
            kind: ShapeKind::Rectangle,
            rect: (200.0, 60.0, 204.0, 64.0),
            shift_constrained: false,
            filled: false,
            apex_high: true,
        },
    );
    st.select_all_of([a, m]);
    assert_eq!(
        st.selection_bounds_on(1),
        Some((0.0, 204.0, 60.0, 64.0)),
        "选框并集应同时覆盖已确认图形与待确认镜像"
    );
    // 镜像的几何为空（空折线）时不参与并集（与已确认图形同规则）
    let empty = st.add_pending(
        1,
        PendingShapeRef::CurvePath(7),
        DrawnShapeSource::Polyline { points: Vec::new() },
    );
    st.select_only(empty);
    assert_eq!(st.selection_bounds_on(1), None);
}
