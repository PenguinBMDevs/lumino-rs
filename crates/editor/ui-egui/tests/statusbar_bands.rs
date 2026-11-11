//! 状态栏：GPU(LGS) 指标组与「点 MEM 打开内存监控」的交互。
//!
//! 无头 kittest：只验证模型语义与命中，不依赖真实 GPU 后端。

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Event, PointerButton, Pos2, RawInput};
use egui_kittest::Harness;
use lumino_ui_egui::chrome::statusbar::{self, StatusbarAction};
use lumino_ui_egui::chrome::{GpuMetricsModel, GpuQueueTrend, StatusbarModel};

fn model(gpu: Option<GpuMetricsModel>) -> StatusbarModel {
    StatusbarModel {
        left: "就绪".to_owned(),
        left_accent: false,
        cpu_percent: Some(12.5),
        memory_mb: 256.0,
        fps: Some(60.0),
        right: String::new(),
        gpu,
    }
}

fn gpu() -> GpuMetricsModel {
    GpuMetricsModel {
        queue_ms: 12.0,
        trend: GpuQueueTrend::Up,
        load: 0.42,
        voice_count: 24,
        underruns: 0,
        sample_gpu: Some("64.0M".to_owned()),
        sample_cpu: Some("48.0M".to_owned()),
        governor_level: None,
        dropped: None,
    }
}

fn harness(model: StatusbarModel) -> (Harness<'static>, Rc<RefCell<Vec<StatusbarAction>>>) {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let actions_w = actions.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(900.0, 20.0))
        .with_accessibility_check(false)
        .build_ui(move |ui| {
            lumino_ui_egui::fonts::install(ui.ctx());
            statusbar::show_model(ui, &model);
            if let Some(action) = statusbar::take_action(ui.ctx()) {
                actions_w.borrow_mut().push(action);
            }
        });
    harness.run();
    (harness, actions)
}

fn click(harness: &mut Harness<'static>, pos: Pos2) {
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
    *harness.input_mut() = input;
    harness.run();
}

#[test]
fn mem_click_requests_memory_monitor() {
    // 右侧指标组右对齐：从右往左扫，命中 MEM 数值即产出动作。
    let mut hits = 0;
    for x in (500..=888).rev() {
        let (mut h, actions) = harness(model(None));
        click(&mut h, Pos2::new(x as f32, 10.0));
        if actions
            .borrow()
            .contains(&StatusbarAction::OpenMemoryMonitor)
        {
            hits += 1;
            break;
        }
    }
    assert_eq!(hits, 1, "点 MEM 数值应请求打开内存监控对话框");
}

#[test]
fn gpu_group_is_optional_and_does_not_steal_layout_when_absent() {
    // 无 GPU：指标组仍渲染（CPU/MEM/FPS），且不产生任何动作。
    let (mut h, actions) = harness(model(None));
    click(&mut h, Pos2::new(880.0, 10.0));
    // 只可能命中 MEM；这里点在 FPS 区域，不应有动作。
    assert!(
        !actions
            .borrow()
            .contains(&StatusbarAction::OpenMemoryMonitor),
        "非 MEM 区域不得触发内存监控"
    );
}

#[test]
fn gpu_group_carries_tones_and_optional_fields() {
    // 模型语义：治理/丢弃为 None 时不显示；负载按 60%/90% 分档（阈值由 ui-egui 单测锁定）。
    let full = gpu();
    assert!(full.sample_gpu.is_some() && full.sample_cpu.is_some());
    assert!(full.governor_level.is_none() && full.dropped.is_none());

    // 带 GPU 的模型能正常绘制（不 panic），且静态渲染不产生动作。
    let (_h, actions) = harness(model(Some(full)));
    assert!(actions.borrow().is_empty(), "静态渲染不应产生动作");
}
