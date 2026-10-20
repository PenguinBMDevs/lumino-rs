//! `ShapeStore` 测试：撤销分组日志、可见性与音符指纹

use super::*;
use lumino_core::shape::{ShapeGeometry, ShapeKind, ShapeStyle};

fn shape_model(id: u64, gate: f32) -> ShapeModel {
    let mut model = ShapeModel::new(id, 0, ShapeKind::Curve, 0.25);
    model.geometry = ShapeGeometry::from_points([(0.0, 60.0), (480.0, 72.0)]);
    model.style = ShapeStyle {
        gate_beats: gate,
        ..ShapeStyle::default()
    };
    model
}

fn note(tick: f32, key: u16) -> ShapeNote {
    ShapeNote {
        track: 0,
        tick,
        key,
        length: 120.0,
        velocity: 100,
    }
}

#[test]
fn test_insert_assigns_and_keeps_ids() {
    let mut store = ShapeStore::new();
    let auto = store.insert(shape_model(0, 0.0625), 0);
    assert_eq!(auto, 1, "id=0 应自动分配");
    let explicit = store.insert(shape_model(42, 0.0625), 0);
    assert_eq!(explicit, 42);
    let next_auto = store.insert(shape_model(0, 0.0625), 0);
    assert_eq!(next_auto, 43, "自动分配应跳过已用 id");
    assert_eq!(store.len(), 3);
    assert_eq!(store.ids(), vec![1, 42, 43]);
}

#[test]
fn test_load_all_does_not_log() {
    let mut store = ShapeStore::new();
    store.load_all(vec![shape_model(1, 0.0625), shape_model(2, 0.0625)]);
    assert_eq!(store.len(), 2);
    assert_eq!(store.log_len(), 0, "加载不是用户操作，不得记日志");
    store.on_undo_group(1);
    assert!(
        store.entry(1).expect("形状应存在").is_visible(),
        "无记录的分组不应改变可见性"
    );
}

#[test]
fn test_clear_resets_ids_and_log() {
    let mut store = ShapeStore::new();
    store.insert(shape_model(0, 0.0625), 7);
    store.insert(shape_model(0, 0.0625), 8);
    store.clear();
    assert!(store.is_empty());
    assert_eq!(store.log_len(), 0);
    assert_eq!(store.insert(shape_model(0, 0.0625), 0), 1, "id 计数应重置");
}

#[test]
fn test_create_undo_hides_and_redo_restores() {
    let mut store = ShapeStore::new();
    let id = store.insert(shape_model(0, 0.0625), 11);
    assert!(store.entry(id).expect("形状应存在").is_visible());

    store.on_undo_group(11);
    assert!(
        !store.entry(id).expect("形状应存在").is_visible(),
        "撤销创建应隐藏"
    );
    assert!(store.persisted_models().is_empty(), "隐藏的形状不写盘");

    store.on_redo_group(11);
    assert!(
        store.entry(id).expect("形状应存在").is_visible(),
        "重做创建应恢复"
    );
    assert_eq!(store.persisted_models().len(), 1);
}

#[test]
fn test_edit_undo_restores_before_and_redo_after() {
    let mut store = ShapeStore::new();
    let id = store.insert(shape_model(0, 0.0625), 0);
    let edited = shape_model(id, 0.5);
    assert!(store.edit(id, edited.clone(), 21));
    assert_eq!(store.model(id).expect("形状应存在").style.gate_beats, 0.5);

    store.on_undo_group(21);
    assert_eq!(
        store.model(id).expect("形状应存在").style.gate_beats,
        0.0625,
        "撤销应回退到前像"
    );

    store.on_redo_group(21);
    assert_eq!(
        store.model(id).expect("形状应存在").style.gate_beats,
        0.5,
        "重做应回到后像"
    );
}

#[test]
fn test_two_edits_undo_independently() {
    let mut store = ShapeStore::new();
    let id = store.insert(shape_model(0, 0.0625), 0);
    store.edit(id, shape_model(id, 0.25), 31);
    store.edit(id, shape_model(id, 0.5), 32);

    store.on_undo_group(32);
    assert_eq!(store.model(id).expect("形状应存在").style.gate_beats, 0.25);
    store.on_undo_group(31);
    assert_eq!(
        store.model(id).expect("形状应存在").style.gate_beats,
        0.0625
    );
    store.on_undo_group(31);
    assert_eq!(
        store.model(id).expect("形状应存在").style.gate_beats,
        0.0625,
        "重复撤销同一分组应幂等"
    );
}

#[test]
fn test_delete_undo_restores_and_redo_deletes() {
    let mut store = ShapeStore::new();
    let id = store.insert(shape_model(0, 0.0625), 0);
    assert!(store.mark_deleted(id, 41));
    assert!(!store.entry(id).expect("形状应存在").is_visible());

    store.on_undo_group(41);
    assert!(
        store.entry(id).expect("形状应存在").is_visible(),
        "撤销删除应恢复"
    );

    store.on_redo_group(41);
    assert!(
        !store.entry(id).expect("形状应存在").is_visible(),
        "重做删除应再次隐藏"
    );
}

#[test]
fn test_redo_without_group_falls_back_to_undo_stack() {
    let mut store = ShapeStore::new();
    let id = store.insert(shape_model(0, 0.0625), 0);
    store.edit(id, shape_model(id, 0.5), 71);
    store.edit(id, shape_model(id, 0.75), 72);

    store.on_undo_group(72);
    assert_eq!(store.model(id).expect("形状应存在").style.gate_beats, 0.5);
    assert_eq!(store.redo_stack_len(), 1);

    // redo 侧丢 group_id：按栈顶兜底回放
    store.on_redo_without_group();
    assert_eq!(
        store.model(id).expect("形状应存在").style.gate_beats,
        0.75,
        "无分组重做应回放最近一次被撤销的变更"
    );
    assert_eq!(store.redo_stack_len(), 0, "兜底回放应消费栈顶");

    // 栈空时的兜底重做是空操作
    store.on_redo_without_group();
    assert_eq!(store.model(id).expect("形状应存在").style.gate_beats, 0.75);
}

#[test]
fn test_redo_group_clears_matching_stack_entry() {
    let mut store = ShapeStore::new();
    let id = store.insert(shape_model(0, 0.0625), 0);
    store.edit(id, shape_model(id, 0.5), 81);

    store.on_undo_group(81);
    assert_eq!(store.redo_stack_len(), 1);
    store.on_redo_group(81);
    assert_eq!(store.model(id).expect("形状应存在").style.gate_beats, 0.5);
    assert_eq!(store.redo_stack_len(), 0, "按分组重做应清掉对应栈项");
}

#[test]
fn test_unknown_group_is_noop() {
    let mut store = ShapeStore::new();
    let id = store.insert(shape_model(0, 0.0625), 0);
    store.edit(id, shape_model(id, 0.5), 51);
    store.on_undo_group(999);
    store.on_redo_group(999);
    assert_eq!(
        store.model(id).expect("形状应存在").style.gate_beats,
        0.5,
        "未知分组不得改变任何形状"
    );
}

#[test]
fn test_edit_or_delete_missing_shape_returns_false() {
    let mut store = ShapeStore::new();
    assert!(!store.edit(123, shape_model(123, 0.25), 1));
    assert!(!store.mark_deleted(123, 1));
    assert!(!store.assign_notes(123, vec![note(0.0, 60)]));
    assert!(store.notes_of(123).is_empty());
}

#[test]
fn test_log_capacity_evicts_oldest() {
    let mut store = ShapeStore::new();
    let id = store.insert(shape_model(0, 0.0625), 0);
    let total = MAX_SHAPE_LOG as u64 + 5;
    for group in 1..=total {
        store.edit(id, shape_model(id, group as f32), group);
    }
    assert_eq!(store.log_len(), MAX_SHAPE_LOG, "日志应被裁剪到上限");

    // 最新分组仍可撤销：回到上一个 gate
    store.on_undo_group(total);
    assert_eq!(
        store.model(id).expect("形状应存在").style.gate_beats,
        (total - 1) as f32
    );

    // 已被淘汰的最旧分组：撤销不到，值不变且不 panic
    let before = store.model(id).expect("形状应存在").style.gate_beats;
    store.on_undo_group(1);
    assert_eq!(
        store.model(id).expect("形状应存在").style.gate_beats,
        before,
        "被淘汰的分组不应再改变形状"
    );
}

#[test]
fn test_persisted_models_excludes_hidden_and_deleted() {
    let mut store = ShapeStore::new();
    let a = store.insert(shape_model(0, 0.0625), 61);
    let b = store.insert(shape_model(0, 0.0625), 62);
    let c = store.insert(shape_model(0, 0.0625), 0);

    store.on_undo_group(61);
    store.on_undo_group(62);
    let persisted = store.persisted_models();
    assert_eq!(persisted.len(), 1, "只有第三个形状应被持久化");
    assert_eq!(persisted[0].id, c);
    assert_eq!(store.len(), 3, "隐藏不等于删除，条目仍在存储中");
    let _ = (a, b);
}

#[test]
fn test_notes_fingerprint_assign_and_read() {
    let mut store = ShapeStore::new();
    let id = store.insert(shape_model(0, 0.0625), 0);
    assert!(store.notes_of(id).is_empty());

    let notes = vec![note(0.0, 60), note(480.0, 62)];
    assert!(store.assign_notes(id, notes.clone()));
    assert_eq!(store.notes_of(id), notes.as_slice());

    store.assign_notes(id, Vec::new());
    assert!(store.notes_of(id).is_empty(), "再生成后应能覆盖旧指纹");
}
