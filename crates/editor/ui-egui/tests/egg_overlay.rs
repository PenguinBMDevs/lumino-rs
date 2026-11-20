//! 「关于」页 logo 彩蛋飞行层：绘制不 panic、不抢指针。
//!
//! 状态机（位置/旋转/透明度）在 `ui-core` 已有 170 行单测；这里只钉住 egui 侧
//! 的两条硬约束：飞行层**不进入占用区**（不吃任何点击）、无模型时零开销。

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Event, PointerButton, Pos2, RawInput};
use egui_kittest::Harness;
use image::RgbaImage;
use lumino_ui_egui::chrome::{self, ChromeModel, DialogModel, EggModel, menu};
use lumino_ui_egui::icons::IconCache;

fn harness(egg: Option<EggModel>) -> (Harness<'static>, Rc<RefCell<Vec<()>>>) {
    let clicks = Rc::new(RefCell::new(Vec::new()));
    let clicks_w = clicks.clone();
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            lumino_ui_egui::fonts::install(ui.ctx());
            chrome::set_egg(ui.ctx(), egg);
            chrome::show_bands(ui, &ChromeModel::default(), &mut icons);
            let _ = ui.ctx().input(|i| i.pointer.any_click()).then(|| {
                clicks_w.borrow_mut().push(());
            });
        });
    harness.run();
    (harness, clicks)
}

fn flying() -> EggModel {
    EggModel {
        x: 320.0,
        y: 180.0,
        box_size: 96.0,
        icon_size: 64.0,
        angle: 0.8,
        opacity: 0.9,
    }
}

#[test]
fn flying_egg_renders_without_blocking_pointer() {
    let (mut h, _) = harness(Some(flying()));
    // 飞行层不是占用区：点它所在位置仍应落到 上一代 / 下层。
    let blocked = menu::take_blocked(&h.ctx);
    assert!(
        blocked.is_empty(),
        "彩蛋飞行层不得进入占用区（否则会吞掉设置页点击）：{blocked:?}"
    );

    // 在 logo 中心点击：不应 panic，也不应产生任何 egui 侧占用。
    let pos = Pos2::new(320.0 + 48.0, 180.0 + 48.0);
    let mut input = RawInput::default();
    input.events.push(Event::PointerMoved(pos));
    input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    });
    input.events.push(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    });
    *h.input_mut() = input;
    h.run();
    assert!(menu::take_blocked(&h.ctx).is_empty());
}

#[test]
fn no_egg_model_is_a_no_op() {
    let (h, _) = harness(None);
    assert!(menu::take_blocked(&h.ctx).is_empty());
}

/// 渲染一帧「设置」独立对话框窗口（`DialogModel::Settings`）。
fn render_settings_dialog(egg: Option<EggModel>) -> RgbaImage {
    let model = ChromeModel {
        dialog: Some(DialogModel::Settings(Box::default())),
        ..ChromeModel::default()
    };
    let mut icons = IconCache::default();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(720.0, 540.0))
        .with_pixels_per_point(1.0)
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            chrome::set_egg(ui.ctx(), egg);
            chrome::show_bands(ui, &model, &mut icons);
        });
    // 图标纹理首帧才上传：多跑一帧保证飞行层进入绘制列表。
    harness.run();
    harness.run();
    harness.render().expect("彩蛋像素测试需要可用的渲染器")
}

/// 回归：设置窗口是独立对话框，`show_bands` 的 dialog 分支会提前 `return`。
/// 飞行层若只画在主窗口的通用覆盖层区，彩蛋序列起飞后**整个飞行都不绘制**，
/// 表现就是「关于页 logo 满连点后直接消失」。
#[test]
fn flying_egg_renders_inside_dialog_windows() {
    let egg = EggModel {
        x: 500.0,
        y: 350.0,
        box_size: 96.0,
        icon_size: 72.0,
        angle: 0.4,
        opacity: 1.0,
    };
    let baseline = render_settings_dialog(None);
    let with_egg = render_settings_dialog(Some(egg));

    // 以方盒为统计区（旋转后的 AABB 仍在方盒内）。
    let left = egg.x as u32;
    let top = egg.y as u32;
    let right = left + egg.box_size as u32;
    let bottom = top + egg.box_size as u32;
    let mut changed = 0u32;
    for y in top..bottom {
        for x in left..right {
            if baseline.get_pixel(x, y) != with_egg.get_pixel(x, y) {
                changed += 1;
            }
        }
    }
    assert!(
        changed > 100,
        "对话框窗口必须绘制彩蛋飞行层（实际变化像素 {changed}）：\
         否则关于页 logo 起飞后会直接消失"
    );
}
