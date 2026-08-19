//! 素材库交互测试：右键菜单 / 重命名 / 删除 / 上传

use std::path::PathBuf;

use lumino_message::MaterialContextMenuItem;

use crate::right_sidebar::MaterialSource;
use crate::root::Root;
use lumino_core::storage::config::UiConfig;

fn create_root() -> Root {
    Root::new(&UiConfig::default())
}

/// 创建唯一临时目录（无 tempfile 依赖，测试后由操作系统清理）
fn make_tmp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lumino_mat_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("临时目录应创建成功");
    dir
}

/// 构造用户素材条目（可指定磁盘路径）
fn user_entry(path: Option<PathBuf>) -> crate::right_sidebar::MaterialEntry {
    crate::right_sidebar::MaterialEntry {
        name: "测试素材".into(),
        author: String::new(),
        source: MaterialSource::User,
        path,
        data: None,
        multi_track: false,
        track_count: 1,
        valid: true,
        preview: None,
    }
}

#[test]
fn test_open_context_menu_sets_target_and_clears_others() {
    let mut root = create_root();
    root.right_sidebar.materials.entries.push(user_entry(None));
    root.right_sidebar.materials.renaming_material = Some((0, "旧名".into()));
    root.right_sidebar.materials.pending_delete = Some(0);
    root.right_sidebar.materials.add_menu_open = true;

    root.open_material_context_menu(0);
    assert_eq!(root.right_sidebar.materials.context_menu_target, Some(0));
    assert!(root.right_sidebar.materials.renaming_material.is_none());
    assert!(root.right_sidebar.materials.pending_delete.is_none());
    assert!(!root.right_sidebar.materials.add_menu_open);
}

#[test]
fn test_open_context_menu_ignores_invalid_index() {
    let mut root = create_root();
    root.open_material_context_menu(99);
    assert!(root.right_sidebar.materials.context_menu_target.is_none());
}

#[test]
fn test_context_menu_rename_starts_inline_edit() {
    let mut root = create_root();
    root.right_sidebar
        .materials
        .entries
        .push(user_entry(Some(PathBuf::from("C:/tmp/a.lmmaterial"))));
    root.open_material_context_menu(0);

    root.handle_material_context_menu_item_clicked(0, MaterialContextMenuItem::Rename);
    assert!(root.right_sidebar.materials.context_menu_target.is_none());
    assert_eq!(
        root.right_sidebar.materials.renaming_material,
        Some((0, "测试素材".into()))
    );
}

#[test]
fn test_context_menu_rename_ignored_for_builtin() {
    // 内置素材无磁盘路径：菜单按钮已置灰，此处验证防御逻辑
    let mut root = create_root();
    root.right_sidebar
        .materials
        .entries
        .push(crate::right_sidebar::MaterialEntry {
            name: "内置".into(),
            author: String::new(),
            source: MaterialSource::BuiltIn,
            path: None,
            data: None,
            multi_track: false,
            track_count: 1,
            valid: true,
            preview: None,
        });
    root.open_material_context_menu(0);

    root.handle_material_context_menu_item_clicked(0, MaterialContextMenuItem::Rename);
    assert!(root.right_sidebar.materials.renaming_material.is_none());
}

#[test]
fn test_rename_input_updates_buffer() {
    let mut root = create_root();
    root.right_sidebar.materials.renaming_material = Some((0, "旧名".into()));
    root.handle_material_rename_input_changed("新名".into());
    assert_eq!(
        root.right_sidebar.materials.renaming_material,
        Some((0, "新名".into()))
    );
}

#[test]
fn test_rename_confirmed_empty_name_rejected() {
    let mut root = create_root();
    root.right_sidebar.materials.renaming_material = Some((0, "   ".into()));
    root.confirm_material_rename();
    // 空名被拒绝：不 panic，编辑态已清除
    assert!(root.right_sidebar.materials.renaming_material.is_none());
}

#[test]
fn test_confirm_rename_missing_entry_noop() {
    let mut root = create_root();
    root.right_sidebar.materials.renaming_material = Some((99, "新名".into()));
    root.confirm_material_rename();
    assert!(root.right_sidebar.materials.renaming_material.is_none());
}

#[test]
fn test_context_menu_delete_enters_confirm_state() {
    let mut root = create_root();
    root.right_sidebar
        .materials
        .entries
        .push(user_entry(Some(PathBuf::from("C:/tmp/a.lmmaterial"))));
    root.open_material_context_menu(0);

    root.handle_material_context_menu_item_clicked(0, MaterialContextMenuItem::Delete);
    assert!(root.right_sidebar.materials.context_menu_target.is_none());
    // 确认态 + 素材名快照（独立对话框窗口展示用）
    assert_eq!(root.right_sidebar.materials.pending_delete, Some(0));
    assert_eq!(
        root.right_sidebar.materials.pending_delete_name.as_deref(),
        Some("测试素材")
    );
}

#[test]
fn test_delete_sets_confirm_snapshot() {
    // 右键删除：确认态 + 素材名快照（覆盖层确认卡片展示用）
    let mut root = create_root();
    root.right_sidebar
        .materials
        .entries
        .push(user_entry(Some(PathBuf::from("C:/tmp/a.lmmaterial"))));
    root.open_material_context_menu(0);
    root.handle_material_context_menu_item_clicked(0, MaterialContextMenuItem::Delete);
    assert_eq!(root.right_sidebar.materials.pending_delete, Some(0));
    assert_eq!(
        root.right_sidebar.materials.pending_delete_name.as_deref(),
        Some("测试素材")
    );
}

#[test]
fn test_cancel_delete_clears_snapshot() {
    let mut root = create_root();
    root.right_sidebar
        .materials
        .entries
        .push(user_entry(Some(PathBuf::from("C:/tmp/a.lmmaterial"))));
    root.open_material_context_menu(0);
    root.handle_material_context_menu_item_clicked(0, MaterialContextMenuItem::Delete);
    root.cancel_material_delete();
    assert!(root.right_sidebar.materials.pending_delete.is_none());
    assert!(root.right_sidebar.materials.pending_delete_name.is_none());
}

#[test]
fn test_confirm_delete_removes_file() {
    let dir = make_tmp_dir();
    let file = dir.join("a.lmmaterial");
    std::fs::write(&file, b"lmpj").expect("写入临时素材失败");

    let mut root = create_root();
    root.right_sidebar
        .materials
        .entries
        .push(user_entry(Some(file.clone())));
    root.right_sidebar.materials.pending_delete = Some(0);

    root.confirm_material_delete(0);
    assert!(!file.exists(), "素材文件应被删除");
    assert!(root.right_sidebar.materials.pending_delete.is_none());
}

#[test]
fn test_confirm_delete_wrong_index_ignored() {
    let dir = make_tmp_dir();
    let file = dir.join("a.lmmaterial");
    std::fs::write(&file, b"lmpj").expect("写入临时素材失败");

    let mut root = create_root();
    root.right_sidebar
        .materials
        .entries
        .push(user_entry(Some(file.clone())));
    root.right_sidebar.materials.pending_delete = Some(0);

    // 索引不匹配：防御性忽略（不删除）
    root.confirm_material_delete(1);
    assert!(file.exists(), "索引不匹配时不应删除文件");
    assert_eq!(root.right_sidebar.materials.pending_delete, Some(0));
}

#[test]
fn test_open_context_menu_snapshots_cursor_pos() {
    let mut root = create_root();
    root.right_sidebar.materials.entries.push(user_entry(None));
    root.right_sidebar.materials.update_cursor_pos(120.0, 80.0);

    root.open_material_context_menu(0);
    // 菜单位置 = 打开瞬间的光标位置快照（面板局部坐标）
    assert_eq!(
        root.right_sidebar.materials.context_menu_pos,
        Some((120.0, 80.0))
    );
    // 菜单打开期间光标移动：弹出位置保持冻结，不跟随鼠标漂移
    root.right_sidebar.materials.update_cursor_pos(300.0, 200.0);
    assert_eq!(
        root.right_sidebar.materials.context_menu_pos,
        Some((120.0, 80.0))
    );
}

#[test]
fn test_close_context_menu_clears_snapshot() {
    let mut root = create_root();
    root.right_sidebar.materials.entries.push(user_entry(None));
    root.right_sidebar.materials.update_cursor_pos(10.0, 20.0);
    root.open_material_context_menu(0);

    root.close_material_context_menu();
    assert!(root.right_sidebar.materials.context_menu_target.is_none());
    assert!(root.right_sidebar.materials.context_menu_pos.is_none());
    // 实时光标位置保留，供下次打开菜单使用
    assert_eq!(
        root.right_sidebar.materials.context_cursor_pos,
        Some((10.0, 20.0))
    );
}

#[test]
fn test_upload_to_cloud_sets_pending_upload_for_user_material() {
    let mut root = create_root();
    let path = PathBuf::from("C:/tmp/素材.lmmaterial");
    root.right_sidebar
        .materials
        .entries
        .push(user_entry(Some(path.clone())));
    root.open_material_context_menu(0);

    root.handle_material_context_menu_item_clicked(0, MaterialContextMenuItem::UploadToCloud);
    let pending = root.cloud.pending_upload.expect("应设置上传待办");
    assert_eq!(pending.local_path, path.to_string_lossy());
    assert_eq!(pending.file_name, "测试素材.lmmaterial");
}

#[test]
fn test_upload_to_cloud_builtin_rejected() {
    // 内置素材不支持上传到云（按钮已置灰，此处验证防御逻辑）
    let mut root = create_root();
    root.right_sidebar
        .materials
        .entries
        .push(crate::right_sidebar::MaterialEntry {
            name: "内置素材".into(),
            author: String::new(),
            source: MaterialSource::BuiltIn,
            path: None,
            data: Some(&[0x4C, 0x4D, 0x50, 0x4A]), // LMPJ
            multi_track: false,
            track_count: 1,
            valid: true,
            preview: None,
        });
    root.open_material_context_menu(0);

    root.handle_material_context_menu_item_clicked(0, MaterialContextMenuItem::UploadToCloud);
    assert!(
        root.cloud.pending_upload.is_none(),
        "内置素材不应设置上传待办"
    );
}

#[test]
fn test_upload_to_cloud_invalid_material_rejected() {
    let mut root = create_root();
    root.right_sidebar
        .materials
        .entries
        .push(crate::right_sidebar::MaterialEntry {
            name: "坏素材".into(),
            author: String::new(),
            source: MaterialSource::BuiltIn,
            path: None,
            data: None,
            multi_track: false,
            track_count: 0,
            valid: false,
            preview: None,
        });
    root.open_material_context_menu(0);

    root.handle_material_context_menu_item_clicked(0, MaterialContextMenuItem::UploadToCloud);
    assert!(
        root.cloud.pending_upload.is_none(),
        "无效素材不应设置上传待办"
    );
}
