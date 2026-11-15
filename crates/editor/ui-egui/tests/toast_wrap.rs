//! Toast 自动折行：中英文长文案都不得溢出卡片。

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use lumino_ui_egui::chrome::{self, ToastCardModel, ToastModel};

const WIDTH: f32 = 900.0;
const HEIGHT: f32 = 600.0;

fn card(message: &str) -> ToastCardModel {
    ToastCardModel {
        icon: "i".to_owned(),
        message: message.to_owned(),
        background: egui::Color32::from_rgb(30, 30, 36),
        text: egui::Color32::WHITE,
    }
}

fn harness(
    model: ToastModel,
    size: egui::Vec2,
) -> (Harness<'static>, Rc<RefCell<Vec<egui::Rect>>>, egui::Rect) {
    let rects = Rc::new(RefCell::new(Vec::new()));
    let rects_w = rects.clone();
    let full = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), size);
    let mut harness = Harness::builder()
        .with_size(size)
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            lumino_ui_egui::fonts::install(ui.ctx());
            chrome::toast::show_toasts(ui, full, &model);
            *rects_w.borrow_mut() = chrome::toast::cached_toast_rects(ui.ctx());
        });
    harness.run();
    (harness, rects, full)
}

#[test]
fn short_message_stays_single_line() {
    let model = ToastModel {
        cards: vec![card("已复制")],
    };
    let (_h, rects, full) = harness(model, egui::vec2(WIDTH, HEIGHT));
    let rects = rects.borrow();
    assert_eq!(rects.len(), 1);
    assert_eq!(rects[0].height(), 34.0, "单行卡片高度不变");
    assert!(rects[0].width() < 420.0, "短文案卡片应收缩：{:?}", rects[0]);
    assert!(full.contains_rect(rects[0]));
}

#[test]
fn long_english_message_wraps() {
    let model = ToastModel {
        cards: vec![card(
            "Shape paste failed: the clipboard does not contain a valid shape share line",
        )],
    };
    let (_h, rects, full) = harness(model, egui::vec2(WIDTH, HEIGHT));
    let rects = rects.borrow();
    assert_eq!(rects.len(), 1);
    let rect = rects[0];
    assert!(rect.width() <= 420.0, "卡片不得超过最大宽度：{rect:?}");
    assert!(
        rect.height() > 34.0,
        "英文长句必须折行（高度应大于单行）：{rect:?}"
    );
    assert!(full.contains_rect(rect), "折行后仍须在窗口内：{rect:?}");
}

#[test]
fn long_chinese_message_wraps() {
    let model = ToastModel {
        cards: vec![card(
            "角度柔化失败：请选中一个待确认折线形状，并确认它至少有三个可柔化的角点",
        )],
    };
    let (_h, rects, full) = harness(model, egui::vec2(WIDTH, HEIGHT));
    let rects = rects.borrow();
    assert_eq!(rects.len(), 1);
    assert!(rects[0].height() > 34.0, "中文长句必须折行：{:?}", rects[0]);
    assert!(full.contains_rect(rects[0]));
}

#[test]
fn multiple_cards_stack_without_overlap() {
    let model = ToastModel {
        cards: vec![
            card("已保存"),
            card("Shape paste failed: the clipboard does not contain a valid shape share line"),
        ],
    };
    let (_h, rects, full) = harness(model, egui::vec2(WIDTH, HEIGHT));
    let rects = rects.borrow();
    assert_eq!(rects.len(), 2);
    // 自下而上：rects[0] 是最下面那张。
    assert!(
        (rects[0].top() - rects[1].bottom() - 6.0).abs() < 0.01,
        "两张卡片间距应为 6：{rects:?}"
    );
    assert!(rects.iter().all(|rect| full.contains_rect(*rect)));
}

#[test]
fn narrow_window_shrinks_card() {
    let size = egui::vec2(320.0, 480.0);
    let model = ToastModel {
        cards: vec![card(
            "Shape paste failed: the clipboard does not contain a valid shape share line",
        )],
    };
    let (_h, rects, full) = harness(model, size);
    let rects = rects.borrow();
    let rect = rects[0];
    assert!(
        rect.width() <= size.x - 32.0,
        "窄窗口下卡片必须收在左右边距内：{rect:?}"
    );
    assert!(rect.height() > 34.0, "收窄后仍须折行：{rect:?}");
    assert!(full.contains_rect(rect));
}
