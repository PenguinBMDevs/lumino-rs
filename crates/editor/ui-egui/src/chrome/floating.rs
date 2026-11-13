//! 浮层图层：把下拉菜单 / 弹出面板画在**独立 egui 图层**上。
//!
//! 背景（为什么不是"画在 root Ui 里"）：
//! egui 的 hover / click 命中按图层决定。同一图层里，后注册的控件虽然在上层，
//! 但**面板空白区（内边距 / 分隔线 / 子菜单间隙）没有控件**，命中会落回下层
//! band（工具栏、左栏、卷帘骨架），表现为「鼠标悬停穿透」——指针明明在菜单上，
//! 下层的按钮却亮了。
//!
//! 这里用 [`egui::Area`] + [`egui::Order::Foreground`] 把整块浮层放进独立图层，
//! 并在面板矩形上注册一个可交互背景：只要存在覆盖指针的命中矩形，egui 就会把
//! 下层图层从命中集合中排除（`hit_test` 的 `included_layers`），穿透随之消失。

use egui::{Context, Id, Order, Rect, Sense, Ui};

/// 在独立图层上绘制浮层面板。
///
/// - `id`：浮层稳定标识（同一浮层跨帧必须一致，否则 Area 状态丢失）；
/// - `rect`：面板矩形（逻辑屏幕坐标，`rect.min` 即图层原点）；
/// - `add`：绘制闭包；返回**超出 `rect` 的附加区域**（如子菜单），
///   返回 `Some(extra)` 时整个并集都会纳入本图层，避免子面板边缘再次穿透。
pub(crate) fn floating_panel(
    ctx: &Context,
    id: &str,
    rect: Rect,
    add: impl FnOnce(&mut Ui) -> Option<Rect>,
) {
    egui::Area::new(Id::unique(id))
        .order(Order::Foreground)
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            ui.set_min_size(rect.size());
            // 面板背景必须可点击：内边距 / 分隔线上的点击与悬停都由本层消费，
            // 不再落到下层的工具栏或侧栏（行控件在本背景之后注册，仍优先命中）。
            let _ = ui.interact(rect, Id::unique((id, "bg")), Sense::click());

            if let Some(extra) = add(ui) {
                let union = rect.union(extra);
                // 子面板用 hover 命中扩展图层范围：不抢 click（否则会吞掉
                // 子面板内行的点击），但足以让 `hit_test` 排除下层图层。
                let _ = ui.interact(union, Id::unique((id, "union")), Sense::hover());
                ui.set_min_size(union.size());
            }
        });
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use egui::{Id, Rect, Sense, pos2, vec2};
    use egui_kittest::Harness;

    /// 下层控件在浮层背景上不得 hover（这正是「菜单穿透」的回归门禁）。
    #[test]
    fn floating_panel_blocks_hover_on_lower_layer() {
        let under_hovered = Rc::new(Cell::new(true));
        let under = under_hovered.clone();
        let mut harness = Harness::builder()
            .with_size(vec2(400.0, 300.0))
            .with_accessibility_check(false)
            .build_ui(move |ui| {
                // 模拟被浮层盖住的工具栏按钮。
                let probe = Rect::from_min_size(pos2(0.0, 30.0), vec2(300.0, 60.0));
                let response = ui.interact(probe, Id::unique("under-probe"), Sense::click());
                under.set(response.hovered());

                let panel = Rect::from_min_size(pos2(40.0, 40.0), vec2(120.0, 80.0));
                super::floating_panel(ui.ctx(), "test-panel", panel, |_ui| None);
            });
        harness.run();
        harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(pos2(100.0, 80.0)));
        // 命中使用上一帧的控件矩形：跑两帧让浮层进入命中集合。
        harness.run();
        harness.run();
        assert!(
            !under_hovered.get(),
            "浮层背景必须独占命中，下层控件不得 hover"
        );
    }

    /// 浮层之外的指针仍应命中下层（不能整窗拦截）。
    #[test]
    fn floating_panel_does_not_block_outside_its_rect() {
        let under_hovered = Rc::new(Cell::new(false));
        let under = under_hovered.clone();
        let mut harness = Harness::builder()
            .with_size(vec2(400.0, 300.0))
            .with_accessibility_check(false)
            .build_ui(move |ui| {
                let probe = Rect::from_min_size(pos2(0.0, 30.0), vec2(300.0, 60.0));
                let response = ui.interact(probe, Id::unique("under-probe"), Sense::click());
                under.set(response.hovered());

                let panel = Rect::from_min_size(pos2(40.0, 40.0), vec2(120.0, 80.0));
                super::floating_panel(ui.ctx(), "test-panel", panel, |_ui| None);
            });
        harness.run();
        // 面板右侧、probe 内部。
        harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(pos2(250.0, 60.0)));
        harness.run();
        harness.run();
        assert!(
            under_hovered.get(),
            "浮层矩形之外的指针必须继续命中下层控件"
        );
    }
}
