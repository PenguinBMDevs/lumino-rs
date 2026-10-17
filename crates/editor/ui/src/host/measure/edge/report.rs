//! 尺子的两个出口：诊断报告（人看）与 CI 门槛（机器判）

use super::super::dialog_sizes::{WORK_AREA, headless_renderer};
use super::corners::{Case, describe, scan_all};

/// 诊断报告：打印全部角落（17 类 × 各自视图状态）的边缘可见性现状
#[test]
#[ignore = "诊断报告；需要时用 --ignored 运行"]
fn report_edge_visibility() {
    let Some(renderer) = headless_renderer() else {
        eprintln!("跳过：无可用 GPU 适配器（边缘可见性诊断需要真实文本度量）");
        return;
    };

    let ui_config = lumino_core::storage::config::UiConfig::default();
    let results = scan_all(&renderer, &ui_config);
    let clean = results
        .iter()
        .filter(|case| case.violations.is_empty())
        .count();

    println!("REF-001 边缘控件可见性诊断（工作区 {WORK_AREA:?}）:");
    for case in &results {
        let Case {
            name,
            window_size,
            node,
            violations,
        } = case;

        if violations.is_empty() {
            println!(
                "  {name}: 干净（窗口 {:.0}×{:.0}）",
                window_size.width, window_size.height
            );
            continue;
        }

        println!(
            "  {name}: {} 处违规（窗口 {:.0}×{:.0}）",
            violations.len(),
            window_size.width,
            window_size.height
        );
        for violation in violations.iter().take(6) {
            let path = node
                .as_ref()
                .map_or_else(String::new, |node| describe(node, violation.index));
            println!(
                "      {axis:?} {kind:?}: 自然 {expected:.1} → 实际 {actual:.1}",
                axis = violation.axis,
                kind = violation.kind,
                expected = violation.expected,
                actual = violation.actual,
            );
            println!("        路径 {path}");
        }
    }

    println!("  —— {} 个角落中 {clean} 个干净", results.len());
}

/// CI 门槛：所有对话框的**所有视图状态**都不许有边缘可见性违规
///
/// 「所有最边缘的控件都显示出来」的可执行定义。遍历维度是
/// **对话框类型 × 视图状态**（设置面板 10 页、协作 4 态、视频导出覆盖层 5 态、
/// 云存储 4 态、找回音轨 2 态）——只测默认状态会漏检，那正是一次验收反馈。
#[test]
fn assert_edge_visibility_clean() {
    let Some(renderer) = headless_renderer() else {
        eprintln!("跳过：无可用 GPU 适配器");
        return;
    };

    let ui_config = lumino_core::storage::config::UiConfig::default();
    let mut failures = Vec::new();

    for case in scan_all(&renderer, &ui_config) {
        let Case {
            name,
            node,
            violations,
            ..
        } = case;

        if violations.is_empty() {
            continue;
        }

        let detail = violations
            .iter()
            .take(3)
            .map(|violation| {
                let path = node
                    .as_ref()
                    .map_or_else(String::new, |node| describe(node, violation.index));
                format!(
                    "{axis:?} {kind:?} 自然 {expected:.1} → 实际 {actual:.1} @ {path}",
                    axis = violation.axis,
                    kind = violation.kind,
                    expected = violation.expected,
                    actual = violation.actual,
                )
            })
            .collect::<Vec<_>>()
            .join("；");

        failures.push(format!("{name}: {} 处（{detail}）", violations.len()));
    }

    assert!(
        failures.is_empty(),
        "以下位置存在边缘控件被压窄/被裁：\n  {}",
        failures.join("\n  ")
    );
}
