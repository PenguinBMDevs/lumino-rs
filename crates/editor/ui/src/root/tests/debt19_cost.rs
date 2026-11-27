//! DEBT-19 诊断口径：同工具重选 / 消息轮询 / 待确认镜像重建的 CPU 成本。
//!
//! 只用于人工采集基线（`--ignored --nocapture`），**不做 CI 断言**——机器差异会
//! 让绝对阈值失真。快捷键体感是否达标仍以真机 profile 为准。
//!
//! ```text
//! cargo test -p lumino-ui --offline -- --ignored --nocapture debt19
//! ```

use std::time::Instant;

use lumino_editor_state::{ShapeInstance, ShapeKind};
use lumino_midi_model::{MidiDocument, NoteEvent};

use super::*;

/// 基线工程规模：2 轨 × 25 万音符 = 50 万音符（与 #270 验收口径一致）。
const NOTES_PER_TRACK: usize = 250_000;
/// 单次测量迭代数。
const ITERS: u32 = 20_000;

/// 50 万音符的基线工程（当前轨 = 1）。
fn root_with_500k_notes() -> Root {
    let mut doc = MidiDocument::empty_with_tracks(2, 480);
    doc.tempo_changes = vec![(0, 120.0)];
    doc.time_signatures = vec![(0, 4, 4)];
    for i in 0..NOTES_PER_TRACK {
        let start = (i % 7680) as u32;
        doc.insert_note(1, NoteEvent::new(start, start + 10, 60, 100, 0));
    }

    let mut root = Root::new(&lumino_core::storage::config::UiConfig::default());
    root.set_midi_document(doc);
    root.editor.editor_state.data.current_track = 1;
    root
}

#[test]
#[ignore = "DEBT-19 基线诊断：手动运行并用 --nocapture 查看 CPU 成本"]
fn debt19_cost_baseline() {
    let mut root = root_with_500k_notes();
    let same_tool = crate::toolbar::Event::tool_selected(crate::toolbar::Tool::Pencil);
    root.update(same_tool.clone());

    // ① 同工具 ToolSelected 的消息处理（不含绘制/渲染）
    let started = Instant::now();
    for _ in 0..ITERS {
        root.update(same_tool.clone());
    }
    println!(
        "[DEBT-19] 同工具 ToolSelected：{:?}/次（{ITERS} 次，50 万音符）",
        started.elapsed() / ITERS
    );

    // ② route_message 的 7 个 poll（空闲工程：应全部早退）
    let started = Instant::now();
    for _ in 0..ITERS {
        root.poll_midi_input();
        root.poll_pending_i2m();
        root.poll_material_scan();
        root.poll_material_import();
        root.poll_winmm_scan();
        root.poll_midi_devices_scan();
        root.poll_path_dialogs();
    }
    println!(
        "[DEBT-19] route_message 的 7 个 poll：{:?}/轮（空闲）",
        started.elapsed() / ITERS
    );

    // ③ 待确认镜像重建（同工具重选曾无条件支付的那一笔）
    root.editor.set_tool(crate::toolbar::Tool::ShapeSelect);
    root.editor
        .editor_state
        .shape_tool
        .shapes
        .push(ShapeInstance {
            kind: ShapeKind::Rectangle,
            rect: (0.0, 60.0, 1920.0, 64.0),
            shift_constrained: false,
            filled: false,
            apex_high: true,
            mask: None,
        });
    root.editor.rebuild_pending_mirrors();
    let started = Instant::now();
    for _ in 0..ITERS {
        root.editor.rebuild_pending_mirrors();
    }
    println!(
        "[DEBT-19] rebuild_pending_mirrors（1 个待确认矩形）：{:?}/次",
        started.elapsed() / ITERS
    );

    // ④ 每帧构建的 egui 外壳模型（与按键无关，但是「按键 → 重绘」的固定成本）
    let frames = 2_000;
    let started = Instant::now();
    for _ in 0..frames {
        let _ = root.egui_chrome_model_for(1280.0);
    }
    println!(
        "[DEBT-19] egui_chrome_model_for：{:?}/帧（50 万音符）",
        started.elapsed() / frames
    );
}
