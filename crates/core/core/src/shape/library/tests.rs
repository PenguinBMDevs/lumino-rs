//! 形状库加载/保存/容错测试（DRAW-033A）

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::shape::{DrawingParams, ShapeKind, ShapeOp, ShapeRect, ShapeStyle, StrokeRole};

fn test_dir(tag: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "lumino_shape_lib_{tag}_{}_{}",
        std::process::id(),
        stamp
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("创建测试目录失败");
    dir
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

fn circle_drawing() -> ShapeDrawing {
    BuiltinShape::Circle.drawing()
}

#[test]
fn test_builtin_library_contains_three_shapes() {
    let library = ShapeLibrary::with_builtins();
    assert_eq!(library.builtin.len(), 3);
    assert!(library.find("circle").is_some());
    assert!(library.find("square").is_some());
    assert!(library.find("triangle").is_some());
    assert!(library.user.is_empty());
}

#[test]
fn test_save_and_load_user_drawing_roundtrip_with_cjk_name() {
    let dir = test_dir("roundtrip");
    let mut drawing = circle_drawing();
    drawing.id = "user-circle".to_string();
    drawing.name = "圆形".to_string();
    save_drawing(&dir, &drawing).expect("保存用户图形失败");

    let (library, warnings) = ShapeLibrary::load_dir(&dir);
    assert!(warnings.is_empty(), "正常文件不应有 warning：{warnings:?}");
    assert_eq!(library.user.len(), 1);
    assert_eq!(library.user[0], drawing, "库文件应逐字段往返一致");
    cleanup(&dir);
}

#[test]
fn test_load_dir_skips_bad_files_and_keeps_good_ones() {
    let dir = test_dir("bad_file");
    let good = BuiltinShape::Square.drawing();
    let mut user = good;
    user.id = "good".to_string();
    save_drawing(&dir, &user).expect("保存好文件失败");
    std::fs::write(dir.join("broken.lmshape"), b"{ not json").expect("写入坏文件失败");

    let (library, warnings) = ShapeLibrary::load_dir(&dir);
    assert_eq!(library.user.len(), 1, "坏文件应跳过，好文件仍加载");
    assert_eq!(library.user[0].id, "good");
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].path.ends_with("broken.lmshape"));
    cleanup(&dir);
}

#[test]
fn test_load_dir_renames_user_id_colliding_with_builtin() {
    let dir = test_dir("builtin_collision");
    let mut drawing = BuiltinShape::Square.drawing();
    drawing.id = "square".to_string();
    drawing.name = "333".to_string();
    save_drawing(&dir, &drawing).expect("保存失败");

    let (library, warnings) = ShapeLibrary::load_dir(&dir);
    assert_eq!(library.builtin.len(), 3);
    assert_eq!(library.user.len(), 1);
    assert_eq!(
        library.user[0].id, "square-2",
        "用户 id 不得遮蔽内置 square"
    );
    assert!(library.find("square").is_some());
    assert_eq!(library.find("square").expect("内置").name, "Square");
    assert_eq!(library.find("square-2").expect("用户").name, "333");
    assert!(
        !dir.join("square.lmshape").exists(),
        "冲突文件应迁移为 square-2.lmshape"
    );
    assert!(dir.join("square-2.lmshape").exists());
    assert_eq!(warnings.len(), 1);
    cleanup(&dir);
}

#[test]
fn test_load_dir_renames_duplicate_ids_without_overwriting() {
    let dir = test_dir("duplicate");
    let mut first = BuiltinShape::Triangle.drawing();
    first.id = "same".to_string();
    first.name = "first".to_string();
    let bytes = encode_drawing(&first).expect("编码失败");
    std::fs::write(dir.join("a.lmshape"), &bytes).expect("写 a 失败");
    std::fs::write(dir.join("b.lmshape"), &bytes).expect("写 b 失败");

    let (library, warnings) = ShapeLibrary::load_dir(&dir);
    assert_eq!(library.user.len(), 2, "重复 id 不得丢文件");
    let ids: Vec<&str> = library
        .user
        .iter()
        .map(|drawing| drawing.id.as_str())
        .collect();
    assert!(ids.contains(&"same"));
    assert!(ids.contains(&"same-2"));
    assert_eq!(warnings.len(), 1, "重复 id 应回传 warning");
    cleanup(&dir);
}

#[test]
fn test_save_overwrites_atomically_without_leftovers() {
    let dir = test_dir("overwrite");
    let mut drawing = BuiltinShape::Circle.drawing();
    drawing.id = "same".to_string();
    drawing.name = "first".to_string();
    save_drawing(&dir, &drawing).expect("首次保存失败");
    drawing.name = "second".to_string();
    save_drawing(&dir, &drawing).expect("覆盖保存失败");

    let (library, warnings) = ShapeLibrary::load_dir(&dir);
    assert!(warnings.is_empty());
    assert_eq!(library.user[0].name, "second");
    let leftovers: Vec<String> = std::fs::read_dir(&dir)
        .expect("读取目录")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".tmp-") || name.ends_with(".bak"))
        .collect();
    assert!(leftovers.is_empty(), "不应残留临时/备份文件：{leftovers:?}");
    cleanup(&dir);
}

#[test]
fn test_delete_drawing_and_missing_dir() {
    let dir = test_dir("delete");
    let mut drawing = BuiltinShape::Triangle.drawing();
    drawing.id = "to-delete".to_string();
    save_drawing(&dir, &drawing).expect("保存失败");
    assert!(delete_drawing(&dir, "to-delete").expect("删除失败"));
    assert!(!delete_drawing(&dir, "to-delete").expect("重复删除应返回 false"));
    let (library, warnings) = ShapeLibrary::load_dir(&dir);
    assert!(library.user.is_empty());
    assert!(warnings.is_empty());
    cleanup(&dir);

    let missing = dir.join("does-not-exist");
    let (library, warnings) = ShapeLibrary::load_dir(&missing);
    assert!(library.user.is_empty());
    assert!(warnings.is_empty(), "目录不存在应视为空库而非错误");
}

#[test]
fn test_save_invalid_drawing_does_not_write_file() {
    let dir = test_dir("invalid");
    let mut drawing = BuiltinShape::Circle.drawing();
    drawing.id = "empty".to_string();
    drawing.strokes.clear();
    assert!(save_drawing(&dir, &drawing).is_err());
    assert!(
        std::fs::read_dir(&dir).expect("读取目录").next().is_none(),
        "非法图形不得留下文件"
    );
    cleanup(&dir);
}

#[test]
fn test_drawing_path_blocks_traversal() {
    let dir = PathBuf::from("base");
    let path = drawing_path(&dir, "../escape");
    assert_eq!(path.parent(), Some(dir.as_path()));
    assert_eq!(
        path.file_name().and_then(|name| name.to_str()),
        Some("escape.lmshape")
    );
}

#[test]
fn test_to_model_maps_unit_box_to_target_rect() {
    let drawing = BuiltinShape::Square.drawing();
    let model = drawing
        .to_model(1, 0.25, ShapeRect::normalized(0.0, 60.0, 480.0, 64.0))
        .expect("转换失败");
    assert_eq!(model.kind, ShapeKind::Custom);
    assert_eq!(model.track, 1);
    assert_eq!(model.name, "Square");
    assert_eq!(
        model.geometry.rect,
        Some(ShapeRect::normalized(0.0, 60.0, 480.0, 64.0))
    );
    assert!(
        model.geometry.closed,
        "单条闭合 stroke 的 Drawing 应标记闭合，供区域着色识别"
    );
    let drawing_op = model
        .ops
        .iter()
        .find_map(|op| match op {
            ShapeOp::Drawing(params) => Some(params),
            _ => None,
        })
        .expect("模型必须含 Drawing 算子");
    assert_eq!(drawing_op.strokes.len(), 1);
    let anchors = &drawing_op.strokes[0].anchors;
    assert_eq!(anchors.first().expect("首点").pos, (0.0, 60.0));
    assert_eq!(anchors[1].pos, (480.0, 60.0));
    assert_eq!(anchors[2].pos, (480.0, 64.0));
    assert_eq!(anchors.last().expect("尾点").pos, (0.0, 60.0));
    assert_eq!(model.style.areas, drawing.areas);
}

#[test]
fn test_to_model_rejects_invalid_target_or_k() {
    let drawing = circle_drawing();
    let bad_rect = ShapeRect {
        tick_lo: 10.0,
        tick_hi: 0.0,
        ..ShapeRect::normalized(0.0, 0.0, 1.0, 1.0)
    };
    assert!(drawing.to_model(1, 0.25, bad_rect).is_err());
    let good_rect = ShapeRect::normalized(0.0, 60.0, 480.0, 64.0);
    assert!(drawing.to_model(1, 0.0, good_rect).is_err(), "k 必须为正");
}

#[test]
fn test_drawing_params_bbox_and_closed_detection() {
    let drawing = BuiltinShape::Triangle.drawing();
    let params = DrawingParams {
        strokes: drawing.strokes.clone(),
        roles: drawing.roles.clone(),
    };
    assert_eq!(
        params.bbox(),
        Some(ShapeRect::normalized(0.0, 0.0, 1.0, 1.0))
    );
    assert!(params.stroke_closed(0));

    let mut open = drawing.strokes[0].clone();
    open.anchors.pop();
    open.rect = None;
    let params = DrawingParams {
        strokes: vec![open],
        roles: vec![StrokeRole::default()],
    };
    assert!(!params.stroke_closed(0));
    assert_eq!(
        params.roles.len(),
        1,
        "角色数量应与 stroke 数量一致以通过 validate"
    );
}

#[test]
fn test_to_model_preserves_style_areas_and_cjk_name() {
    let mut drawing = BuiltinShape::Circle.drawing();
    drawing.id = "cjk".to_string();
    drawing.name = "圆形".to_string();
    drawing.areas = vec![crate::shape::AreaSpot {
        u: 0.5,
        v: 0.5,
        colour: 3,
    }];
    let model = drawing
        .to_model(2, 1.0, ShapeRect::normalized(0.0, 60.0, 960.0, 72.0))
        .expect("转换失败");
    assert_eq!(model.name, "圆形");
    assert_eq!(
        model.style,
        ShapeStyle {
            fill: crate::shape::ShapeFill::Fill,
            areas: drawing.areas.clone(),
            ..ShapeStyle::default()
        }
    );
}
