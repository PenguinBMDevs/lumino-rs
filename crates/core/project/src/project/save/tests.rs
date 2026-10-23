//! 工程保存测试
//!
//! 从 `save.rs` 拆出（REF-001：单文件 ≤400 行，含测试不豁免）。

use super::*;
use crate::project::data_formats::LmshapesData;
use crate::{FolderPaths, LmtrackData, TrackMeta, TrackVisibilitySer};
use lumino_core::shape::{ShapeGeometry, ShapeKind, ShapeModel};
use lumino_midi_model::compact::{CompactEvent, EventKind};

fn create_test_project() -> LuminoProject {
    let mut project = LuminoProject::new("Test Project");
    let data = LmtrackData::from_compact_events(
        TrackMeta {
            track_id: 0,
            name: "Piano".into(),
            channel: 0,
            port: 0,
            visibility: TrackVisibilitySer::Visible,
            solo: false,
            is_drum: false,
            max_tick: 1000,
        },
        &[
            CompactEvent::new(0, 0, EventKind::NoteOn, 0, 60, 100),
            CompactEvent::new(480, 0, EventKind::NoteOff, 0, 60, 0),
        ],
    );
    project.add_track(data);
    project
}

/// S0-2 形状夹具：一个几何合法的曲线形状
fn shape_sample(id: u64) -> ShapeModel {
    let mut shape = ShapeModel::new(id, 0, ShapeKind::Curve, 0.25);
    shape.geometry = ShapeGeometry::from_points([(0.0, 60.0), (480.0, 72.0)]);
    shape.validate().expect("测试形状应通过校验");
    shape
}

/// S0-2：非空形状写出 `.lmshapes`（魔数 LMSH），且可原样往返
#[test]
fn test_shapes_roundtrip_folder() {
    let mut project = create_test_project();
    project.shapes = vec![shape_sample(1), shape_sample(2)];
    let dir = atomic_test_dir("shapes_folder");
    save_to_folder(&project, &dir).expect("保存工程失败");

    let shapes_path = dir.join(FolderPaths::SHAPES_FILE);
    assert!(shapes_path.exists(), "非空形状应写出 .lmshapes 部件");
    let bytes = std::fs::read(&shapes_path).expect("读取形状部件失败");
    assert_eq!(&bytes[0..4], b"LMSH", "形状部件魔数应为 LMSH");

    let loaded = crate::load_project(&dir).expect("加载工程失败");
    assert_eq!(loaded.shapes, project.shapes, "形状应原样往返");

    let _ = std::fs::remove_dir_all(&dir);
}

/// DRAW-040 / D5–D7：改动前生成的 v1 夹具 → 保存恒写 v2 → 读回逐字段一致
#[test]
fn test_shapes_v1_fixture_saves_as_v2() {
    const V1_FIXTURE: &[u8] = include_bytes!("../load/fixtures/shapes_v1.lmshapes");
    let v1 = LmshapesData::decode(V1_FIXTURE).expect("v1 夹具应能解码");

    let mut project = create_test_project();
    project.shapes = v1.shapes;
    let dir = atomic_test_dir("shapes_v1_to_v2");
    save_to_folder(&project, &dir).expect("保存工程失败");

    let bytes = std::fs::read(dir.join(FolderPaths::SHAPES_FILE)).expect("读取形状部件失败");
    assert_eq!(&bytes[0..4], b"LMSH", "形状部件魔数应为 LMSH");
    assert_eq!(
        u16::from_le_bytes([bytes[4], bytes[5]]),
        2,
        "保存必须恒写版本 2"
    );

    let loaded = crate::load_project(&dir).expect("加载工程失败");
    assert_eq!(loaded.shapes, project.shapes, "v2 读回应与保存前逐字段一致");

    let _ = std::fs::remove_dir_all(&dir);
}

/// S0-2：空形状**不写**部件——否则老工程保存后字节会变
#[test]
fn test_empty_shapes_omits_folder_part() {
    let project = create_test_project();
    let dir = atomic_test_dir("shapes_empty_folder");
    save_to_folder(&project, &dir).expect("保存工程失败");

    assert!(
        !dir.join(FolderPaths::SHAPES_FILE).exists(),
        "空形状不得写出 .lmshapes 部件"
    );
    let loaded = crate::load_project(&dir).expect("加载工程失败");
    assert!(loaded.shapes.is_empty(), "缺失部件应为空形状列表");

    let _ = std::fs::remove_dir_all(&dir);
}

/// S0-2：归档形态非空时含部件，且可往返
#[test]
fn test_shapes_roundtrip_archive() {
    let mut project = create_test_project();
    project.shapes = vec![shape_sample(9)];
    let path = atomic_test_dir("shapes_archive").join("project.lmpj");
    save_to_archive(&project, &path).expect("保存归档失败");
    let bytes = std::fs::read(&path).expect("读取归档失败");

    assert!(
        crate::read_file_from_archive(&bytes, FolderPaths::SHAPES_FILE)
            .expect("读取归档部件失败")
            .is_some(),
        "归档应包含形状部件"
    );
    let loaded = crate::load_project(&path).expect("加载归档失败");
    assert_eq!(loaded.shapes, project.shapes, "形状应原样往返");
}

/// S0-2：空形状归档不含部件，且同一工程两次保存字节一致（确定性）
#[test]
fn test_empty_shapes_archive_has_no_part_and_is_deterministic() {
    let project = create_test_project();
    let path = atomic_test_dir("shapes_empty_archive").join("a.lmpj");
    save_to_archive(&project, &path).expect("保存归档失败");
    let first = std::fs::read(&path).expect("读取归档失败");

    assert!(
        crate::read_file_from_archive(&first, FolderPaths::SHAPES_FILE)
            .expect("读取归档部件失败")
            .is_none(),
        "空形状不得在归档中写出部件"
    );

    save_to_archive(&project, &path).expect("重复保存失败");
    let second = std::fs::read(&path).expect("读取归档失败");
    assert_eq!(first, second, "同一工程两次保存应字节一致");
}

/// S0-2：含未知部件的工程仍可加载（旧版本读新工程不报错）
#[test]
fn test_archive_with_unknown_part_still_loads() {
    let project = create_test_project();
    let meta = build_metadata(&project)
        .to_toml_str()
        .expect("元数据序列化失败");
    let files = vec![
        ("metadata.toml".to_string(), meta.into_bytes(), true),
        (".lumino/version".to_string(), b"1".to_vec(), false),
        (
            "data/project/future.lmzz".to_string(),
            vec![1, 2, 3, 4],
            false,
        ),
    ];
    let bytes = crate::build_archive(&files).expect("构建归档失败");

    let loaded =
        crate::project::load::load_project_from_bytes(&bytes).expect("含未知部件的工程应能加载");
    assert!(loaded.shapes.is_empty(), "未知部件不影响形状（缺失即空）");
}

#[test]
fn test_save_to_folder() {
    let project = create_test_project();
    let save_dir = std::env::temp_dir().join("lumino_save_folder_test");
    let _ = std::fs::remove_dir_all(&save_dir);

    save_to_folder(&project, &save_dir).expect("保存项目到文件夹失败");

    assert!(save_dir.join("metadata.toml").exists());
    assert!(save_dir.join(".lumino/version").exists());
    assert!(save_dir.join("data/project/tracks/000.lmtrack").exists());
    assert!(save_dir.join("data/project/tempo.lmtemp").exists());
    assert!(save_dir.join("data/project/signature.lmsig").exists());
    assert!(save_dir.join("data/project/controls.lmctl").exists());
    assert!(save_dir.join("data/project/text_events.lmtxt").exists());
    assert!(save_dir.join("data/project/sysex.lmsyx").exists());
    assert!(save_dir.join("data/project/track_names.lmnames").exists());

    // 验证魔数
    let tempo_bytes =
        std::fs::read(save_dir.join("data/project/tempo.lmtemp")).expect("读取tempo文件失败");
    assert_eq!(&tempo_bytes[0..4], b"LMTM");

    let sig_bytes = std::fs::read(save_dir.join("data/project/signature.lmsig"))
        .expect("读取signature文件失败");
    assert_eq!(&sig_bytes[0..4], b"LMSG");

    let ctl_bytes =
        std::fs::read(save_dir.join("data/project/controls.lmctl")).expect("读取controls文件失败");
    assert_eq!(&ctl_bytes[0..4], b"LMCT");

    let txt_bytes = std::fs::read(save_dir.join("data/project/text_events.lmtxt"))
        .expect("读取text_events文件失败");
    assert_eq!(&txt_bytes[0..4], b"LMTX");

    let syx_bytes =
        std::fs::read(save_dir.join("data/project/sysex.lmsyx")).expect("读取sysex文件失败");
    assert_eq!(&syx_bytes[0..4], b"LMSY");

    let names_bytes = std::fs::read(save_dir.join("data/project/track_names.lmnames"))
        .expect("读取track_names文件失败");
    assert_eq!(&names_bytes[0..4], b"LMNM");

    let _ = std::fs::remove_dir_all(&save_dir);
}

#[test]
fn test_save_to_archive() {
    let project = create_test_project();
    let save_archive_path = std::env::temp_dir().join("lumino_save_archive_test.lmpj");
    let _ = std::fs::remove_file(&save_archive_path);

    save_to_archive(&project, &save_archive_path).expect("保存项目到归档失败");

    assert!(save_archive_path.exists());
    let bytes = std::fs::read(&save_archive_path).expect("读取归档文件失败");
    assert!(bytes.len() > 4);
    assert_eq!(&bytes[0..4], b"LMPJ");

    let _ = std::fs::remove_file(&save_archive_path);
}

fn atomic_test_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("lumino_save_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("创建测试目录失败");
    dir
}

fn assert_no_leftovers(dir: &std::path::Path) {
    let leftovers: Vec<String> = std::fs::read_dir(dir)
        .expect("读取测试目录失败")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp-") || n.ends_with(".bak"))
        .collect();
    assert!(leftovers.is_empty(), "不应残留临时/备份文件: {leftovers:?}");
}

/// DEBT-01 #118：覆盖保存必须原子替换——旧目标被整体换掉，不留 tmp/bak。
#[test]
fn test_save_to_archive_atomic_replace() {
    let dir = atomic_test_dir("archive_replace");
    let target = dir.join("project.lmpj");
    std::fs::write(&target, b"stale-not-an-archive").expect("写入旧内容失败");

    let project = create_test_project();
    save_to_archive(&project, &target).expect("原子保存归档失败");

    let bytes = std::fs::read(&target).expect("读取归档失败");
    assert_eq!(&bytes[0..4], b"LMPJ", "旧内容必须被完整替换");
    assert!(
        archive::read_file_from_archive(&bytes, "metadata.toml")
            .expect("读取归档内容失败")
            .is_some(),
        "替换后的归档必须可读"
    );
    assert_no_leftovers(&dir);
    let _ = std::fs::remove_dir_all(&dir);
}

/// DEBT-01 #118：文件夹保存整体换入，旧目录的陈旧文件不得残留。
#[test]
fn test_save_to_folder_atomic_replace() {
    let dir = atomic_test_dir("folder_replace");
    let target = dir.join("project.lmpj");
    std::fs::create_dir_all(&target).expect("创建旧目录失败");
    std::fs::write(target.join("stale.bin"), b"stale").expect("写入陈旧文件失败");

    let project = create_test_project();
    save_to_folder(&project, &target).expect("原子保存文件夹失败");

    assert!(target.join("metadata.toml").exists());
    assert!(
        !target.join("stale.bin").exists(),
        "旧目录的陈旧文件必须整体消失"
    );
    assert_no_leftovers(&dir);
    let _ = std::fs::remove_dir_all(&dir);
}
