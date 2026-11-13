//! 音频导出进度对话框的无头测试：只读内容 + 进度条比例。

use egui_kittest::Harness;
use lumino_ui_egui::chrome::{
    self, ChromeModel, DialogModel, ExportMessageKind, ExportProgressModel,
};
use lumino_ui_egui::icons::IconCache;

fn model(kind: ExportMessageKind, progress: f32) -> ExportProgressModel {
    ExportProgressModel {
        titlebar: false,
        title: "音频导出".to_owned(),
        message: match kind {
            ExportMessageKind::Normal => "正在渲染第 3 段".to_owned(),
            ExportMessageKind::Error => "导出失败：磁盘已满".to_owned(),
            ExportMessageKind::Completed => "导出完成".to_owned(),
        },
        kind,
        progress,
        percentage: lumino_ui_egui::export_progress::percentage_text(progress as f64),
    }
}

fn make_harness(model: ExportProgressModel) -> Harness<'static> {
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(420.0, 200.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            let model = ChromeModel {
                dialog: Some(DialogModel::ExportProgress(Box::new(model.clone()))),
                ..ChromeModel::default()
            };
            chrome::show_bands(ui, &model, &mut icons);
        });
    harness.run();
    harness
}

/// 标题、状态行与百分比都进入可访问树；越界进度夹紧到 100%。
#[test]
fn labels_are_visible_and_progress_is_clamped() {
    use egui_kittest::kittest::Queryable;

    let harness = make_harness(model(ExportMessageKind::Normal, 0.331));
    for label in ["音频导出", "正在渲染第 3 段", "33.1%"] {
        assert!(
            harness.query_by_label(label).is_some(),
            "缺少可访问标签：{label}"
        );
    }

    let harness = make_harness(model(ExportMessageKind::Error, 1.2));
    assert!(harness.query_by_label("导出失败：磁盘已满").is_some());
    assert!(harness.query_by_label("100.0%").is_some());
}

/// 进度条按比例填充：左半为强调色，右半保持轨道色。
#[test]
fn progress_bar_fills_by_ratio() {
    use lumino_ui_egui::theme;

    let mut harness = make_harness(model(ExportMessageKind::Completed, 0.5));
    let image = harness.render().expect("需要可用的 wgpu 渲染器");
    // 内边距 24 + 进度条顶边 84；条高 30、宽 = 画布宽 - 48。
    let y = 24 + 84 + 15;
    let x_left = 24 + 20;
    let x_right = 420 - 24 - 20;
    let left = image.get_pixel(x_left, y).0;
    let right = image.get_pixel(x_right, y).0;
    assert_eq!(
        &left[..3],
        &[
            theme::accent().r(),
            theme::accent().g(),
            theme::accent().b()
        ],
        "已完成的一半应是强调色"
    );
    assert_eq!(
        &right[..3],
        &[
            theme::selected_bg().r(),
            theme::selected_bg().g(),
            theme::selected_bg().b()
        ],
        "未完成的一半应是轨道色"
    );
}
