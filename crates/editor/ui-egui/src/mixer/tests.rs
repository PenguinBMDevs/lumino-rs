//! 混音台几何 / 命中 / 动作通道单测。

use egui::Rect;

use super::*;
use crate::layout;

fn model(n: usize) -> MixerModel {
    MixerModel {
        scroll_x: 0.0,
        strips: (0..n)
            .map(|i| MixerStrip {
                name: format!("T{i}"),
                volume: 0.5,
                pan: 0.0,
                muted: false,
                soloed: false,
                meter: 0.0,
                master: false,
            })
            .collect(),
        master: Some(MixerStrip {
            name: "Master".to_string(),
            volume: 0.5,
            pan: 0.0,
            muted: false,
            soloed: false,
            meter: 0.0,
            master: true,
        }),
    }
}

fn rect() -> Rect {
    Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(720.0, 240.0))
}

#[test]
fn dialog_body_starts_below_title_and_header() {
    let m = model(2);
    let layout = MixerLayout::dialog(rect(), true, &m);
    assert!((layout.panel.top() - 0.0).abs() < 0.01);
    assert!(
        (layout.body.top() - (layout::TITLEBAR_H + HEADER_H)).abs() < 0.01,
        "主体必须让出 30px 标题栏 + 内容标题行"
    );
    let strip = layout.strip_rect(0, 0.0);
    assert!((strip.top() - (layout.body.top() + BODY_PAD)).abs() < 0.01);
    assert!((strip.height() - FADER_HEIGHT).abs() < 0.01);
}

#[test]
fn master_is_pinned_and_excluded_from_strip_viewport() {
    let m = model(2);
    let layout = MixerLayout::dialog(rect(), false, &m);
    let master = layout.master.expect("主控条应存在");
    assert!((master.right() - (layout.panel.right() - BODY_PAD)).abs() < 0.01);
    assert!(
        layout.strips_viewport.right() <= master.left(),
        "普通通道条视口不得与主控条重叠"
    );
}

#[test]
fn strip_geometry_uses_step_and_scroll() {
    let m = model(3);
    let layout = MixerLayout::dialog(rect(), false, &m);
    let r0 = layout.strip_rect(0, 0.0);
    let r1 = layout.strip_rect(1, 0.0);
    assert!((r1.left() - r0.left() - STRIP_STEP).abs() < 0.01);
    let r1s = layout.strip_rect(1, 20.0);
    assert!((r1s.left() - (r1.left() - 20.0)).abs() < 0.01);
}

#[test]
fn visible_culls_by_viewport_with_margin() {
    let m = model(20);
    let layout = MixerLayout::dialog(
        Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(260.0, 240.0)),
        false,
        &m,
    );
    let vis = layout.visible(20, 0.0);
    assert!(vis.contains(&0) && vis.contains(&1), "前两条必须可见");
    assert!(!vis.contains(&19), "远端条应被裁剪");
}

#[test]
fn max_scroll_keeps_last_strip_inside_viewport() {
    let m = model(10);
    let layout = MixerLayout::dialog(rect(), false, &m);
    let max = layout.max_scroll(10);
    let last = layout.strip_rect(9, max);
    assert!(
        last.right() <= layout.strips_viewport.right() - BODY_PAD + 0.01,
        "滚到底时最后一条必须完整落在视口内"
    );
}

#[test]
fn hit_test_maps_fader_buttons_and_master() {
    let m = model(1);
    let layout = MixerLayout::dialog(rect(), false, &m);
    let strip = layout.strip_rect(0, 0.0);
    assert_eq!(
        hit_test(
            egui::pos2(strip.center().x, strip.top() + 60.0),
            &layout,
            &m
        ),
        Some(MixerHit::Fader(0))
    );
    let third = strip.width() / 3.0;
    assert_eq!(
        hit_test(
            egui::pos2(strip.left() + third * 1.5, strip.bottom() - 24.0),
            &layout,
            &m
        ),
        Some(MixerHit::Mute(0))
    );
    assert_eq!(
        hit_test(
            egui::pos2(strip.left() + third * 2.5, strip.bottom() - 24.0),
            &layout,
            &m
        ),
        Some(MixerHit::Solo(0))
    );
    let master = layout.master.expect("主控条");
    assert_eq!(
        hit_test(master.center(), &layout, &m),
        Some(MixerHit::MasterVolume)
    );
}

#[test]
fn action_round_trip_preserves_order() {
    let ctx = egui::Context::default();
    push_action(&ctx, MixerAction::Mute(0));
    push_action(&ctx, MixerAction::Solo(1));
    assert_eq!(
        take_actions(&ctx),
        vec![MixerAction::Mute(0), MixerAction::Solo(1)]
    );
    assert!(take_actions(&ctx).is_empty(), "取走后不得残留");
}
