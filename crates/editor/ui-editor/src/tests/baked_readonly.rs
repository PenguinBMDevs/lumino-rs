//! DEBT-15（#266）：已 Bake 形状的全部回写入口负向测试（DEC-001）
//!
//! 每个历史入口对「已 Bake 选区」都必须返回拒绝，且文档音符 / ShapeStore
//! 模型 / 算子链逐条不变。入口实现分散在各模块，本文件做一张总清单，
//! 防止后续新增路径绕过 DEC-001。

use crate::Editor;
use crate::Note;
use crate::tests::test_helpers::seed_notes;
use lumino_core::shape::{ShapeFill, ShapeGeometry, ShapeKind, ShapeModel, ShapeStyle};
use lumino_editor_state::shape_ops::SymReject;
use lumino_editor_state::{DrawnShapeSource, ShapeNote};

/// 已 Bake 矩形 + 绑定的 ShapeStore 模型（音符由测试直接 seed）
fn editor_with_baked_rect() -> (Editor, u64, u64) {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[Note::new(0.0, 60, 10.0)]);
    editor.editor_state.data.current_track = 1;
    editor.editor_state.tool = lumino_message::Tool::ShapeSelect;

    let mut model = ShapeModel::new(0, 1, ShapeKind::Custom, 0.25);
    let mut geometry =
        ShapeGeometry::from_points([(0.0, 60.0), (480.0, 60.0), (480.0, 64.0), (0.0, 64.0)]);
    geometry.closed = true;
    model.geometry = geometry;
    model.style = ShapeStyle {
        fill: ShapeFill::Fill,
        ..ShapeStyle::default()
    };
    let model_id = editor.editor_state.shape_store.insert(model, 0);

    let drawn_id = editor.editor_state.shape_select.add(
        1,
        None,
        DrawnShapeSource::Polyline {
            points: vec![(0.0, 60.0), (480.0, 60.0), (480.0, 64.0), (0.0, 64.0)],
        },
        vec![ShapeNote {
            track: 1,
            tick: 0.0,
            key: 60,
            length: 10.0,
            velocity: 100,
            channel: 0,
        }],
    );
    editor
        .editor_state
        .shape_select
        .set_model_id(drawn_id, Some(model_id));
    editor.editor_state.shape_select.select_only(drawn_id);
    (editor, drawn_id, model_id)
}

/// 文档音符 + ShapeStore 音符的指纹。
type Fingerprint = (Vec<(u32, u8, u32)>, Vec<(f32, u16, f32)>);

/// 文档音符 + ShapeStore 音符 + 模型算子链的指纹
fn fingerprint(editor: &Editor, model_id: u64) -> Fingerprint {
    let doc = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .map(|note| (note.start_tick, note.key, note.end_tick))
        .collect();
    let store = editor
        .editor_state
        .shape_store
        .notes_of(model_id)
        .iter()
        .map(|note| (note.tick, note.key, note.length))
        .collect();
    (doc, store)
}

#[test]
fn test_all_single_selection_entries_reject_baked_shape() {
    let (mut editor, drawn_id, model_id) = editor_with_baked_rect();
    let before = fingerprint(&editor, model_id);
    let ops_before = editor
        .editor_state
        .shape_store
        .model(model_id)
        .expect("模型")
        .ops
        .clone();

    // 020 编辑图形
    assert!(!editor.enable_shape_edit(), "已 Bake 不得进入编辑图形");
    assert!(!editor.begin_shape_edit(0.0, 64.0), "旧锚点不得命中");
    // 032B 区域着色
    assert!(!editor.begin_area_paint(), "已 Bake 不得进入区域着色");
    assert!(!editor.begin_area_paint_model(model_id), "列表模型不得着色");
    // 010 遮罩
    assert_eq!(editor.mask_edit_target(), None, "已 Bake 无遮罩目标");
    assert!(!editor.toggle_mask_selected(), "已 Bake 不得切换遮罩");
    // 025 切割 / 饼干模
    assert!(!editor.slice_selected_vertical(), "已 Bake 不得切割");
    assert_eq!(
        editor.mould_selected(),
        Err(crate::MouldReject::Selection),
        "已 Bake 不得饼干模"
    );
    // 044 柔化
    assert!(!editor.begin_round_corners(), "已 Bake 不得柔化");
    // 027 / 028
    assert!(!editor.copy_along_selected(), "已 Bake 不得沿路径复制");
    assert!(!editor.velocity_up_selected(), "已 Bake 不得力度渐强");
    // 023 / 024
    assert!(!editor.set_straighten(true), "已 Bake 不得扶正");
    assert_eq!(
        editor.toggle_symmetry(true),
        Err(SymReject::NoSelection),
        "已 Bake 不得对称"
    );
    // 019/021 拆分
    assert!(!editor.split_composed_shape(), "已 Bake 不提供拆分");

    assert_eq!(fingerprint(&editor, model_id), before, "拒绝后音符必须不变");
    assert_eq!(
        editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型")
            .ops,
        ops_before,
        "拒绝后算子链必须不变"
    );
    assert!(
        editor.editor_state.shape_select.is_selected(drawn_id),
        "拒绝不得改变选中集"
    );
}

#[test]
fn test_all_multi_selection_entries_reject_baked_shapes() {
    let (mut editor, first, model_id) = editor_with_baked_rect();
    let second = editor.editor_state.shape_select.add(
        1,
        None,
        DrawnShapeSource::Polyline {
            points: vec![(960.0, 60.0), (1200.0, 64.0)],
        },
        vec![ShapeNote {
            track: 1,
            tick: 960.0,
            key: 60,
            length: 10.0,
            velocity: 100,
            channel: 0,
        }],
    );
    editor
        .editor_state
        .shape_select
        .select_all_of([first, second]);
    let before = fingerprint(&editor, model_id);

    assert!(!editor.live_selected_shapes(), "已 Bake 不得折叠");
    assert!(!editor.join_selected_shapes(), "已 Bake 不得连接");
    assert!(!editor.merge_selected_shapes(), "已 Bake 不得合并");
    assert!(!editor.create_between_selected(), "已 Bake 不得补间");

    assert_eq!(fingerprint(&editor, model_id), before, "拒绝后音符必须不变");
    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        2,
        "拒绝后选中集不得变化"
    );
}
