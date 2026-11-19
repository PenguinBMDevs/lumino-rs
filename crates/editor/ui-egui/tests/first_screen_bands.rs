//! 第一屏（无工程）：左右分栏 + 圆角方块按钮的像素与点击回归。
//!
//! 覆盖用户契约：
//! - 无工程时右栏必须有 5 个操作按钮（新建 / 打开文件 / 打开文件夹 / 设置 / 协作）；
//! - 有工程时只显示品牌块（右栏不得残留按钮）；
//! - 每个按钮点击后产生对应动作（动作由宿主翻译成菜单同一批事件）。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use image::RgbaImage;
use lumino_ui_egui::chrome::first_screen::{
    self, FirstScreenAction, FirstScreenButton, FirstScreenModel,
};
use lumino_ui_egui::chrome::{self, ChromeModel};
use lumino_ui_egui::icons::IconCache;

const WIDTH: u32 = 1200;
const HEIGHT: u32 = 800;
/// 状态栏高度（内容区 = 窗口高 − 状态栏）。
const STATUSBAR_H: u32 = 20;
const BUTTON_COUNT: usize = 5;

fn welcome_model() -> FirstScreenModel {
    let button = |action, label: &str| FirstScreenButton {
        action,
        label: label.to_owned(),
    };
    FirstScreenModel {
        opacity: 0.2,
        actions: vec![
            button(FirstScreenAction::NewProject, "新建工程"),
            button(FirstScreenAction::OpenProjectFile, "打开工程文件"),
            button(FirstScreenAction::OpenProjectFolder, "打开工程文件夹"),
            button(FirstScreenAction::OpenSettings, "打开设置"),
            button(FirstScreenAction::OpenCollaboration, "连接到多人协作房间"),
        ],
    }
}

fn content_area() -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(0.0, 0.0),
        egui::vec2(WIDTH as f32, (HEIGHT - STATUSBAR_H) as f32),
    )
}

fn click(harness: &mut Harness<'static>, pos: egui::Pos2) {
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::PointerMoved(pos));
    input.events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    input.events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    *harness.input_mut() = input;
    harness.run();
}

/// 像素是否等于指定颜色（含不透明判定）。
fn is_color(image: &RgbaImage, x: u32, y: u32, color: egui::Color32) -> bool {
    let pixel = image.get_pixel(x, y).0;
    pixel[..3] == color.to_array()[..3] && pixel[3] > 0
}

#[test]
fn welcome_screen_layout_pixels_and_clicks() {
    let model = Rc::new(RefCell::new(welcome_model()));
    let model_w = model.clone();
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .wgpu()
        .with_size(egui::vec2(WIDTH as f32, HEIGHT as f32))
        .with_pixels_per_point(1.0)
        .with_render_cursor(false)
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::set_first_screen(ui.ctx(), Some(model_w.borrow().clone()));
            chrome::show_bands(ui, &ChromeModel::default(), &mut icons);
            if let Some(action) = first_screen::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    let image = harness
        .render()
        .expect("第一屏像素测试需要可用的 wgpu 渲染器");

    // ── 几何：与绘制同源（split / button_column / button_rect）──
    let area = content_area();
    let (left, right) = first_screen::split(area);
    let column = first_screen::button_column(right, BUTTON_COUNT).expect("右栏必须放得下 5 个按钮");
    assert!(
        column.left() > left.right(),
        "按钮列必须在分栏线右侧（左栏只放品牌）"
    );

    // ── 5 个圆角方块按钮：底色逐像素命中（文字居中，取左侧内边距采样）──
    let card = lumino_ui_egui::theme::card_bg();
    for index in 0..BUTTON_COUNT {
        let rect = first_screen::button_rect(column, index);
        let x = (rect.left() + 8.0) as u32;
        let y = rect.center().y as u32;
        assert!(
            is_color(&image, x, y, card),
            "第 {index} 个按钮必须是圆角方块底色（x={x}, y={y}）"
        );
        // 按钮之间是空隙（不是整列铺底）：证明是 5 个独立方块而不是一整条背景。
        if index + 1 < BUTTON_COUNT {
            let gap_y = (rect.bottom() + 6.0) as u32;
            assert_eq!(
                image.get_pixel(x, gap_y).0[3],
                0,
                "第 {index} 与 {} 个按钮之间必须是空隙",
                index + 1
            );
        }
    }

    // ── 左栏必须有品牌块（图标 + 字标），且不受「水印透明度」淡化 ──
    // 模型这里刻意给 0.2（默认水印档）：欢迎屏是功能页，品牌块必须仍是实心。
    let opaque_left = (0..left.width() as u32)
        .flat_map(|x| (0..left.height() as u32).map(move |y| (x, y)))
        .filter(|&(x, y)| image.get_pixel(x, y).0[3] > 0)
        .count();
    let max_alpha_left = (0..left.width() as u32)
        .flat_map(|x| (0..left.height() as u32).map(move |y| (x, y)))
        .map(|(x, y)| image.get_pixel(x, y).0[3])
        .max()
        .unwrap_or(0);
    assert!(
        opaque_left > 1_000,
        "左栏必须绘制品牌块（实际不透明像素 {opaque_left}）"
    );
    assert!(
        max_alpha_left > 200,
        "欢迎屏品牌块不得按 20% 水印淡化（实际最大 alpha {max_alpha_left}）"
    );

    // ── 点击每个按钮：动作必须逐条命中 ──
    let expected = [
        FirstScreenAction::NewProject,
        FirstScreenAction::OpenProjectFile,
        FirstScreenAction::OpenProjectFolder,
        FirstScreenAction::OpenSettings,
        FirstScreenAction::OpenCollaboration,
    ];
    for (index, want) in expected.iter().enumerate() {
        let rect = first_screen::button_rect(column, index);
        click(&mut harness, rect.center());
        assert_eq!(
            actions.borrow().last(),
            Some(want),
            "第 {index} 个按钮点击后必须产生 {want:?}"
        );
    }

    // ── 有工程：只显示品牌块，右栏不得残留按钮 ──
    *model.borrow_mut() = FirstScreenModel::brand_only(1.0);
    harness.run();
    let image = harness
        .render()
        .expect("第一屏像素测试需要可用的 wgpu 渲染器");
    for index in 0..BUTTON_COUNT {
        let rect = first_screen::button_rect(column, index);
        let x = (rect.left() + 8.0) as u32;
        let y = rect.center().y as u32;
        assert!(
            !is_color(&image, x, y, card),
            "有工程时第 {index} 个按钮位置不得再画按钮"
        );
    }
}
