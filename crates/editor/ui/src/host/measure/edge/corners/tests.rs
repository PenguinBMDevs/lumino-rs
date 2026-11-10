//! **覆盖是不是假的**：每个声明出来的状态都必须真的改变布局。
//!
//! 先例：设置面板只扫第 0 页时报告"17/17 干净"——覆盖清单里写着一个角落，
//! 实际渲染的却是同一份内容，尺子绿得毫无意义。这个测试盯的就是那种假绿：
//! 状态设置函数写错字段名、字段被视图忽略、两种状态渲染等价，都会在这里变红。

use super::super::layout_at_size;
use super::*;
use crate::host::measure::dialog_sizes::headless_renderer;

/// 布局指纹：先序节点的整数化尺寸表
fn signature(
    renderer: &Renderer,
    dialog_type: DialogType,
    apply: &Setup,
    ui_config: &lumino_core::storage::config::UiConfig,
) -> Vec<(u32, u32)> {
    let mut root = Root::new_dialog_with_config(&ui_config.theme, dialog_type, ui_config);
    apply(&mut root);

    let window_size = decide_dialog_window_size(&root, renderer, dialog_type, WORK_AREA);
    let node = layout_at_size(root.view(), renderer, window_size);

    let mut profile = Vec::new();
    collect(&node, &mut profile);
    profile
}

fn collect(node: &Node, out: &mut Vec<(u32, u32)>) {
    let bounds = node.bounds();
    out.push((bounds.width as u32, bounds.height as u32));

    for child in node.children() {
        collect(child, out);
    }
}

#[test]
fn every_declared_state_actually_changes_the_layout() {
    let Some(renderer) = headless_renderer() else {
        eprintln!("跳过：无可用 GPU 适配器");
        return;
    };

    let ui_config = lumino_core::storage::config::UiConfig::default();
    let mut failures = Vec::new();

    for dialog_type in ALL_DIALOGS {
        let states = dialog_states(dialog_type);

        let Some((_, reference)) = states.first() else {
            failures.push(format!("{dialog_type:?}: 状态清单为空"));
            continue;
        };

        // iced 视图已退化为整窗占位的对话框：状态不再影响布局，比较无意义
        // （状态覆盖由 egui 模型测试保证，见 `layout_is_state_invariant`）。
        if layout_is_state_invariant(dialog_type) {
            // 仍然逐个渲染一遍：保留「每种状态都能排出来」的冒烟覆盖。
            for (_, apply) in &states {
                let _ = signature(&renderer, dialog_type, apply, &ui_config);
            }
            continue;
        }

        let reference = signature(&renderer, dialog_type, reference, &ui_config);

        for (suffix, apply) in states.iter().skip(1) {
            if signature(&renderer, dialog_type, apply, &ui_config) == reference {
                failures.push(format!(
                    "{dialog_type:?}{suffix}: 布局与默认状态逐节点相同——\
                     状态没写进正确的字段，或视图忽略了它，这个角落的覆盖是假的"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "以下角落的状态没有真的改变布局：\n  {}",
        failures.join("\n  ")
    );
}

/// 人工排查：打印多状态对话框**每种状态**的窗口尺寸（钉死尺寸前的取证）。
///
/// 运行：`cargo test -p lumino-ui --lib debug_dump_multistate_sizes -- --ignored --nocapture`
#[test]
#[ignore = "人工排查工具，需要时用 --ignored 显式运行"]
fn debug_dump_multistate_sizes() {
    use crate::host::measure::decide_dialog_window_size;

    let Some(renderer) = headless_renderer() else {
        eprintln!("跳过：无可用 GPU 适配器");
        return;
    };
    let ui_config = lumino_core::storage::config::UiConfig::default();
    for dialog_type in ALL_DIALOGS {
        let states = dialog_states(dialog_type);
        if states.len() <= 1 {
            continue;
        }
        for (label, _) in &states {
            let Some(root) = root_for(dialog_type, label, &ui_config) else {
                continue;
            };
            let size = decide_dialog_window_size(&root, &renderer, dialog_type, WORK_AREA);
            println!(
                "SIZE {dialog_type:?}{label} {:.0}x{:.0}",
                size.width, size.height
            );
        }
    }
}
