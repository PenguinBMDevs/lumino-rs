//! 内存监控对话框的无头测试：只读内容 + 关闭动作。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use lumino_ui_egui::chrome::{
    self, ChromeModel, DialogModel, MemoryMetricModel, MemoryMonitorModel, MemoryRowModel,
};
use lumino_ui_egui::icons::IconCache;
use lumino_ui_egui::memory_monitor::{
    MemoryMonitorAction, PREFERRED_CLIENT_HEIGHT, close_button_rect, content_rect,
};

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
        // 生产说明的完整长度（折行 3 行），是内容高度的最坏输入之一。
        note: "注：GPU 资源计数反映应用显式创建的 wgpu Texture/Buffer 大小；驱动层额外开销（swapchain、depth、pipeline cache 等）不纳入此项统计。".to_owned(),
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

/// 一条被绘制的文本：包围盒 + 生效裁剪矩形。
struct DrawnText {
    rect: egui::Rect,
    clip: egui::Rect,
    text: String,
}

/// 按真实对话框尺寸跑一帧，收集全部文本形状（含滚动视口的裁剪矩形）。
fn drawn_texts(model: MemoryMonitorModel, width: f32, height: f32) -> Vec<DrawnText> {
    let ctx = egui::Context::default();
    let mut icons = IconCache::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width, height),
        )),
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        let model = ChromeModel {
            dialog: Some(DialogModel::MemoryMonitor(Box::new(model.clone()))),
            ..ChromeModel::default()
        };
        chrome::show_bands(ui, &model, &mut icons);
    });
    // 测试不接渲染后端：字体图集增量必须丢弃，否则 epaint 析构报错。
    output.textures_delta.clear();

    let mut out = Vec::new();
    for clipped in &output.shapes {
        collect_text(&clipped.shape, clipped.clip_rect, &mut out);
    }
    out
}

fn collect_text(shape: &egui::Shape, clip: egui::Rect, out: &mut Vec<DrawnText>) {
    match shape {
        egui::Shape::Text(text) => out.push(DrawnText {
            rect: egui::Rect::from_min_size(text.pos, text.galley.size()),
            clip,
            text: text.galley.text().to_owned(),
        }),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_text(shape, clip, out);
            }
        }
        _ => {}
    }
}

/// 最坏情况（3 行指标 + 6 行子系统 + 3 行说明）在推荐窗口尺寸下必须
/// **完整放得下**：不需要滚动，也没有任何文字越过按钮带上沿。
///
/// 回归的是本 bug 的根因：窗口高度曾钉死 402，6 行子系统时说明文字底
/// 到 434，直接压在「关闭」按钮上。
#[test]
fn worst_case_content_fits_at_preferred_height() {
    let client =
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, PREFERRED_CLIENT_HEIGHT));
    let content = content_rect(client, true);
    let close = close_button_rect(client, true);

    assert!(
        content.bottom() <= close.top(),
        "内容视口底 {} 不得越过按钮顶 {}",
        content.bottom(),
        close.top()
    );
    assert!(
        close.bottom() <= client.bottom() + 0.5,
        "关闭按钮必须落在客户区内：按钮底 {} / 客户区底 {}",
        close.bottom(),
        client.bottom()
    );

    let texts = drawn_texts(model(6), client.width(), client.height());
    for text in texts.iter().filter(|text| text.text != "关闭") {
        assert!(
            text.rect.bottom() <= content.bottom() + 0.5,
            "内容「{}」底 {:.1} 超出内容视口底 {:.1}：推荐高度装不下最坏内容",
            text.text,
            text.rect.bottom(),
            content.bottom()
        );
    }

    // 防止「说明段根本没被排版」造成的假通过。
    assert!(
        texts
            .iter()
            .any(|text| text.text.starts_with("注：GPU 资源计数")),
        "最坏内容必须包含折行说明段，否则本测试没有覆盖到说明高度"
    );
}

/// 窗口被压得比内容矮时，溢出内容必须被滚动视口裁掉，
/// 可见区域绝不允许与「关闭」按钮相交（结构性保证，不依赖高度数字）。
#[test]
fn overflowing_content_never_overlaps_close_button() {
    let client = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 300.0));
    let close = close_button_rect(client, true);
    let texts = drawn_texts(model(6), client.width(), client.height());

    assert!(
        !texts.iter().any(|text| text.text == "子系统 5"),
        "300px 高视口必须裁掉底部子系统行（本测试需要真实的溢出场景）"
    );
    assert!(
        texts.iter().any(|text| text.text == "关闭"),
        "关闭按钮在任何高度下都必须可见"
    );
    for text in texts.iter().filter(|text| text.text != "关闭") {
        let visible = text.rect.intersect(text.clip);
        assert!(
            !visible.intersects(close),
            "内容「{}」可见区域 {visible:?} 与关闭按钮 {close:?} 重叠",
            text.text
        );
    }
}
