//! 内存监控对话框的无头测试：只读内容 + 关闭动作。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{
    self, ChromeModel, DialogModel, MemoryMetricModel, MemoryMonitorModel, MemoryRowModel,
};
use lumino_ui_egui::icons::IconCache;
use lumino_ui_egui::memory_monitor::MemoryMonitorAction;

fn model(rows: usize) -> MemoryMonitorModel {
    MemoryMonitorModel {
        titlebar: false,
        title: "内存占用详情".to_owned(),
        metrics: vec![
            MemoryMetricModel {
                label: "分配器追踪总内存".to_owned(),
                value: "123.4 MB".to_owned(),
            },
            MemoryMetricModel {
                label: "系统 RSS".to_owned(),
                value: "256.7 MB".to_owned(),
            },
            MemoryMetricModel {
                label: "GPU 资源".to_owned(),
                value: "64.0 MB".to_owned(),
            },
        ],
        grid_title: "按子系统分类".to_owned(),
        rows: (0..rows)
            .map(|index| MemoryRowModel {
                name: format!("子系统 {index}"),
                value: format!("{}.0 MB", index + 1),
            })
            .collect(),
        empty: "（暂无子系统数据）".to_owned(),
        note: "注：GPU 资源计数反映应用显式创建的 wgpu Texture/Buffer 大小".to_owned(),
        close: "关闭".to_owned(),
    }
}

fn make_harness(
    model: MemoryMonitorModel,
) -> (Harness<'static>, Rc<RefCell<Vec<MemoryMonitorAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(520.0, 460.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::MemoryMonitor(Box::new(model.clone()))),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
            if let Some(action) = lumino_ui_egui::memory_monitor::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

#[test]
fn metrics_and_rows_are_visible() {
    let (harness, _) = make_harness(model(3));
    for label in [
        "内存占用详情",
        "分配器追踪总内存",
        "123.4 MB",
        "系统 RSS",
        "256.7 MB",
        "GPU 资源",
        "64.0 MB",
        "按子系统分类",
        "子系统 0",
        "1.0 MB",
        "子系统 2",
        "3.0 MB",
        "关闭",
    ] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }
}

#[test]
fn close_button_dispatches() {
    let (mut harness, actions) = make_harness(model(2));
    harness.get_by_label("关闭").click();
    harness.run();
    assert_eq!(actions.borrow().last(), Some(&MemoryMonitorAction::Close));
}

#[test]
fn empty_state_is_visible() {
    let (harness, _) = make_harness(model(0));
    assert!(harness.query_by_label("（暂无子系统数据）").is_some());
}
