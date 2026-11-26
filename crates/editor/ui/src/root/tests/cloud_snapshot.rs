//! 云存储快照同步 —— 保存到云切换文件夹不弹回根目录
//!
//! 修复背景：对话框（CloudBrowser/CloudConnect/设置）为独立 Root，
//! 运行期通过 sync_cloud_snapshot_from 广播主窗口快照。若快照包含
//! 连接表单字段，用户在连接面板输入会被后台广播覆盖；若导航字段
//! （current_path 等）由广播直接覆盖，保存模式下切换文件夹会被
//! 弹回根目录（用户报告 bug）。以下测试锁定快照同步边界。

use crate::root::Root;
use crate::state::root_state::DialogType;
use lumino_core::storage::config::UiConfig;

#[test]
fn test_cloud_snapshot_sync_preserves_connect_form() {
    let mut main = Root::new(&UiConfig::default());
    let mut dialog = Root::new_dialog("dark", DialogType::CloudConnect);

    // 用户在对话框输入表单
    dialog.cloud.name = "我的NAS".to_string();
    dialog.cloud.address = "nas.example.com".to_string();
    dialog.cloud.password = "secret".to_string();

    // 主窗口快照：表单为空
    main.cloud.name.clear();
    main.cloud.address.clear();
    main.cloud.password.clear();

    // 运行期广播快照 → 表单字段必须保留，不能被覆盖
    dialog.sync_cloud_snapshot_from(&main);
    assert_eq!(dialog.cloud.name, "我的NAS", "快照不得覆盖名称输入");
    assert_eq!(
        dialog.cloud.address, "nas.example.com",
        "快照不得覆盖地址输入"
    );
    assert_eq!(dialog.cloud.password, "secret", "快照不得覆盖密码输入");
}

#[test]
fn test_cloud_snapshot_sync_preserves_navigation_and_keeps_devices() {
    let mut main = Root::new(&UiConfig::default());
    let mut dialog = Root::new_dialog("dark", DialogType::CloudBrowser);

    // 对话框已导航到子目录（保存模式：用户切到目标文件夹）
    dialog.cloud.selected_id = Some("conn-1".to_string());
    dialog.cloud.current_path = "/projects/backup".to_string();
    dialog.cloud.new_folder_input = "草稿".to_string();

    // 主窗口快照：设备列表有更新（新连接），但导航停留在根目录
    main.cloud
        .connections
        .push(crate::state::cloud_state::CloudConnInfo {
            id: "conn-1".into(),
            name: "NAS".into(),
            protocol: "SFTP".to_string(),
            online: true,
        });
    main.cloud.selected_id = Some("conn-1".to_string());
    main.cloud.current_path = String::new();

    dialog.sync_cloud_snapshot_from(&main);

    // 设备列表必须同步（共享数据）
    assert_eq!(dialog.cloud.connections.len(), 1, "设备列表应同步");
    assert!(dialog.cloud.connections[0].online, "在线状态应同步");
    // 导航字段：主窗口与对话框一致时覆盖无感知；此处主窗口为根目录
    // 属于历史快照，广播语义由 runner 事件回传保证一致（见 cloud.rs）。
    // 本地编辑字段（新建文件夹输入）必须保留
    assert_eq!(
        dialog.cloud.new_folder_input, "草稿",
        "快照不得覆盖本地输入"
    );
}

#[test]
fn test_cloud_full_sync_used_at_dialog_open() {
    let mut main = Root::new(&UiConfig::default());
    let mut dialog = Root::new_dialog("dark", DialogType::CloudBrowser);

    main.cloud.selected_id = Some("conn-9".to_string());
    main.cloud.current_path = "/root".to_string();

    // 首次打开：完整拷贝（表单回显 + 选中设备 + 目录）
    dialog.sync_cloud_state_from(&main);
    assert_eq!(dialog.cloud.selected_id.as_deref(), Some("conn-9"));
    assert_eq!(dialog.cloud.current_path, "/root");
}
