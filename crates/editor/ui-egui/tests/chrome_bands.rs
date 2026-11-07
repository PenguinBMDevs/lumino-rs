//! 外壳 band 的无头渲染测试：真实状态模型 → 可访问节点（菜单 / 状态栏文本）。
//!
//! 只验证「状态进来、文本出去」的接线与可见性规则；像素级对齐由
//! `bridge_parity`（四场景）与真实窗口截图负责。

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{self, ChromeModel, StatusbarModel, TitlebarModel};
use lumino_ui_egui::icons::IconCache;

fn statusbar(fps: Option<f32>) -> StatusbarModel {
    StatusbarModel {
        left: "就绪".to_owned(),
        left_accent: fps.is_some(),
        cpu_percent: fps.map(|_| 12.5),
        memory_mb: 256.0,
        fps,
        right: String::new(),
    }
}

fn harness(model: ChromeModel) -> Harness<'static> {
    let mut icons = IconCache::default();
    Harness::builder()
        .with_size(egui::vec2(960.0, 620.0))
        .build_ui(move |ui| chrome::show_bands(ui, &model, &mut icons))
}

#[test]
fn bands_expose_menu_labels_and_status_metrics() {
    let mut harness = harness(ChromeModel {
        titlebar: Some(TitlebarModel {
            focused: true,
            menus: vec![
                "文件".to_owned(),
                "编辑".to_owned(),
                "视图".to_owned(),
                "帮助".to_owned(),
            ],
        }),
        sidebar: None,
        toolbar: None,
        statusbar: statusbar(Some(58.5)),
    });
    harness.run();

    for label in ["文件", "编辑", "视图", "帮助"] {
        assert!(
            harness.query_by_label(label).is_some(),
            "标题栏菜单 {label} 未出现在可访问树中"
        );
    }
    for text in ["就绪", "CPU", "12.5%", "MEM", "256.0 MB", "FPS", "58.5"] {
        assert!(
            harness.query_by_label(text).is_some(),
            "状态栏文本 {text} 未出现在可访问树中"
        );
    }
}

#[test]
fn native_titlebar_skips_top_band_but_keeps_statusbar() {
    let mut harness = harness(ChromeModel {
        titlebar: None,
        sidebar: None,
        toolbar: None,
        statusbar: statusbar(Some(60.0)),
    });
    harness.run();

    assert!(
        harness.query_by_label("文件").is_none(),
        "系统原生标题栏模式下不该绘制 egui 菜单"
    );
    assert!(
        harness.query_by_label("就绪").is_some(),
        "标题栏关闭时状态栏仍必须渲染"
    );
}

#[test]
fn missing_fps_hides_metric_group() {
    let mut harness = harness(ChromeModel {
        titlebar: None,
        sidebar: None,
        toolbar: None,
        statusbar: statusbar(None),
    });
    harness.run();

    assert!(harness.query_by_label("就绪").is_some());
    assert!(
        harness.query_by_label("CPU").is_none(),
        "FPS 未采样时生产逻辑隐藏整组指标"
    );
    assert!(harness.query_by_label("FPS").is_none());
}
