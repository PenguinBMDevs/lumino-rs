//! 内存监控：模型镜像与关闭动作映射。

use super::*;
use lumino_ui_egui::chrome::DialogModel;
use lumino_ui_egui::memory_monitor::MemoryMonitorAction;

fn dialog_root() -> Root {
    Root::new_dialog(
        &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
        crate::state::root_state::DialogType::MemoryMonitor,
    )
}

#[test]
fn memory_monitor_model_mirrors_snapshot() {
    let root = dialog_root();
    let model = root.egui_chrome_model();
    let Some(DialogModel::MemoryMonitor(panel)) = model.dialog else {
        panic!("内存监控应由 egui 整窗接管");
    };
    assert!(panel.titlebar, "默认配置使用自制标题栏");
    assert_eq!(panel.title, "内存占用详情");
    assert_eq!(panel.grid_title, "按子系统分类");
    assert_eq!(panel.close, "关闭");
    assert!(!panel.note.is_empty(), "GPU 计数口径说明不得为空");
    assert_eq!(panel.metrics.len(), 3, "总内存 / RSS / GPU 三行常显");
    for metric in &panel.metrics {
        assert!(!metric.label.is_empty());
        assert!(
            metric.value.ends_with(" MB"),
            "指标值应为 MB 格式：{}",
            metric.value
        );
    }
    // 子系统行只包含占用 > 0 的桶（可能为空）。
    for row in &panel.rows {
        assert!(!row.name.is_empty());
        assert!(row.value.ends_with(" MB"), "子系统值应为 MB：{}", row.value);
    }
}

#[test]
fn memory_monitor_close_maps_to_window_event() {
    let root = dialog_root();
    let message = root
        .memory_monitor_action_message(&MemoryMonitorAction::Close)
        .expect("关闭动作应映射为窗口事件");
    // 事件枚举未实现 PartialEq：用 Debug 文本核对动作名。
    let text = format!("{message:?}");
    assert!(
        text.contains("CloseMemoryMonitorDialog"),
        "关闭应映射为 CloseMemoryMonitorDialog，实际 {text}"
    );
}
