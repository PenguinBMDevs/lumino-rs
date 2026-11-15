//! DRAW-047：图层顺序回归 —— 悬浮绘制工具条必须画在画布预览之上。
//!
//! 复现口径：构造一个整块覆盖工具条的待确认矩形（填充 + 描边块），
//! 胶囊内的像素必须与「没有预览」时逐字节一致；同时验证预览确实画出来了
//! （胶囊上方的对照点必须变化），避免用例退化成「什么都没画」也能过。

use egui_kittest::Harness;
use lumino_ui_egui::chrome::{
    self, ChromeModel, StatusbarModel, ToolPanelItemKind, ToolPanelItemModel, ToolPanelModel,
};
use lumino_ui_egui::icons::{Icon, IconCache};
use lumino_ui_egui::shape_preview::{PreviewShape, ShapePreviewModel};

const WIDTH: f32 = 900.0;
const HEIGHT: f32 = 700.0;

fn statusbar() -> StatusbarModel {
    StatusbarModel {
        left: "就绪".to_owned(),
        left_accent: true,
        cpu_percent: Some(1.0),
        memory_mb: 256.0,
        fps: Some(60.0),
        right: String::new(),
        gpu: None,
    }
}

fn tool_panel() -> ToolPanelModel {
    ToolPanelModel {
        offset: (0.0, 44.0),
        left_inset: 0.0,
        top_inset: 100.0,
        thickness_tooltip: "粗细".to_owned(),
        items: vec![ToolPanelItemModel {
            kind: ToolPanelItemKind::Mouse,
            icon: Icon::MousePointer,
            tooltip: "鼠标".to_owned(),
            selected: true,
            settings_request: false,
        }],
        thickness_selected: false,
        panel: None,
    }
}

fn model() -> ChromeModel {
    ChromeModel {
        tool_panel: Some(tool_panel()),
        statusbar: statusbar(),
        ..ChromeModel::default()
    }
}

fn harness() -> Harness<'static> {
    let model = model();
    let mut icons = IconCache::default();
    Harness::builder()
        .with_size(egui::vec2(WIDTH, HEIGHT))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::show_bands(ui, &model, &mut icons);
        })
}

#[test]
fn pending_preview_must_not_cover_tool_panel() {
    let mut harness = harness();
    harness.run();

    let pill = chrome::tool_panel::pill_rect(&harness.ctx).expect("工具条应已绘制");
    let inside = pill.center();
    // 对照点：胶囊正上方 24px，落在同一块预览内但不在胶囊上。
    let above = egui::pos2(inside.x, pill.top() - 24.0);

    let baseline = harness.render().expect("需要可用的 wgpu 渲染器");
    let base_inside = baseline.get_pixel(inside.x as u32, inside.y as u32).0;
    let base_above = baseline.get_pixel(above.x as u32, above.y as u32).0;

    // 整块覆盖工具条（含上方对照点）的待确认矩形：填充 + 描边产物块。
    let cover = egui::Rect::from_min_max(
        egui::pos2(pill.left() - 40.0, pill.top() - 60.0),
        egui::pos2(pill.right() + 40.0, pill.bottom() + 10.0),
    );
    chrome::set_shape_preview(
        &harness.ctx,
        ShapePreviewModel {
            clip: Some(egui::Rect::from_min_max(
                egui::pos2(0.0, 0.0),
                egui::pos2(WIDTH, HEIGHT),
            )),
            shapes: vec![PreviewShape {
                points: vec![
                    cover.left_top(),
                    cover.right_top(),
                    cover.right_bottom(),
                    cover.left_bottom(),
                ],
                filled: true,
                blocks: vec![cover],
            }],
        },
    );
    harness.run();

    let with_preview = harness.render().expect("需要可用的 wgpu 渲染器");
    let inside_after = with_preview.get_pixel(inside.x as u32, inside.y as u32).0;
    let above_after = with_preview.get_pixel(above.x as u32, above.y as u32).0;

    assert_ne!(
        above_after, base_above,
        "预览必须真的画出来（否则本用例的覆盖区域无效）"
    );
    assert_eq!(
        inside_after, base_inside,
        "工具条必须画在画布预览之上（胶囊内像素不得被预览改写）"
    );
}
