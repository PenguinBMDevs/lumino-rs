//! 工具栏宽度预算与折叠行为测试
//!
//! 三条产品口径在这里被钉死（都写成**扫宽度**的不变量，而不是某条像素宽度）：
//!
//! 1. **主工具栏逐项折叠**：可见工具恒为 `ToolItem::ALL` 的前缀、隐藏恒为后缀，
//!    即"按顺序一个一个折"，而不是整组一刀切；
//! 2. **仪表盘晚于主工具栏**：仪表盘被折叠时，工具区必须已整组收起；
//! 3. **「更多」按钮永不被裁**：任何宽度下 `布局宽度 ≤ 内容区宽度`
//!    （折叠时预算里已预留「更多」按钮的位置）。

use super::*;
use crate::toolbar::buttons::TOOL_BUTTON_WIDTH;
use crate::toolbar::overflow::interaction::{TOOLBAR_HORIZONTAL_PADDING, tools_section_width};
use crate::toolbar::overflow::{MORE_BUTTON_WIDTH, MORE_GAP, ToolItem, ToolbarGroup};
use lumino_extras::i18n::Language;

/// 扫描区间：从"连更多按钮都紧巴"到"远超全展开宽度"
const WIDTHS: std::ops::RangeInclusive<u32> = 320..=2400;

/// 宽度充足时不应该有任何折叠，也不该出现「更多」按钮
#[test]
fn test_wide_toolbar_keeps_everything() {
    let toolbar = Toolbar::new();

    let overflow = toolbar.compute_overflow(2400.0, false);
    assert!(!overflow.has_overflow(), "宽窗口不应出现折叠");
    assert_eq!(overflow.visible_groups.as_slice(), ToolbarGroup::ORDER);
    assert_eq!(overflow.visible_tools.as_slice(), ToolItem::ALL);
    assert!(overflow.hidden_groups.is_empty());
    assert!(overflow.hidden_tools.is_empty());
}

/// 可见/隐藏的**顺序**不变量：工具区前缀 vs 后缀、分组保持 ORDER
#[test]
fn test_visible_tools_stay_a_prefix_of_all_items() {
    let toolbar = Toolbar::new();

    for width in WIDTHS {
        let overflow = toolbar.compute_overflow(width as f32, false);
        let visible = overflow.visible_tools.len();

        assert_eq!(
            overflow.visible_tools.as_slice(),
            &ToolItem::ALL[..visible],
            "width={width}: 可见工具必须是完整前缀（渲染顺序不可打乱）"
        );
        assert_eq!(
            overflow.hidden_tools.as_slice(),
            &ToolItem::ALL[visible..],
            "width={width}: 隐藏工具必须是后缀（从右端逐个让位）"
        );

        let mut last_index = None;
        for group in &overflow.visible_groups {
            let index = ToolbarGroup::ORDER
                .iter()
                .position(|candidate| candidate == group)
                .unwrap_or_else(|| panic!("width={width}: 未知分组 {group:?}"));
            if let Some(last) = last_index {
                assert!(index > last, "width={width}: 分组顺序必须保持 ORDER");
            }
            last_index = Some(index);
        }
    }
}

/// 主工具栏**逐项**折叠：存在"只折掉部分按钮、工具区仍在"的中间态
#[test]
fn test_tools_collapse_item_by_item() {
    let toolbar = Toolbar::new();

    let partial = WIDTHS.rev().find(|width| {
        let overflow = toolbar.compute_overflow(*width as f32, false);
        !overflow.hidden_tools.is_empty() && !overflow.tools_group_hidden()
    });
    let width = partial.expect("必须存在逐项折叠的中间态（不是整组一刀切）");

    let overflow = toolbar.compute_overflow(width as f32, false);
    assert_eq!(
        overflow.hidden_tools.first(),
        Some(&ToolItem::Tie),
        "最先让位的应是最右端的工具按钮"
    );
    assert!(
        overflow.visible_tools.contains(&ToolItem::Pointer),
        "指针工具应留到最后"
    );
    assert!(
        !overflow.hidden_tools.is_empty() && overflow.hidden_tools.len() < ToolItem::ALL.len(),
        "该宽度下应只有部分工具被折叠"
    );
}

/// 仪表盘**整组**折叠，且必须晚于主工具栏（含所有工具按钮）
#[test]
fn test_dashboard_collapses_after_tools() {
    let toolbar = Toolbar::new();

    for width in WIDTHS {
        let overflow = toolbar.compute_overflow(width as f32, false);
        if overflow.hidden_groups.contains(&ToolbarGroup::Dashboard) {
            assert!(
                overflow.tools_group_hidden(),
                "width={width}: 仪表盘不得先于主工具栏折叠"
            );
            assert_eq!(
                overflow.hidden_tools.as_slice(),
                ToolItem::ALL,
                "width={width}: 仪表盘折叠时所有工具按钮必须已折叠"
            );
        }
    }

    // 顺序真的生效：存在"主工具栏已整组收起、仪表盘还在"的区间
    let dashboard_kept = WIDTHS.rev().find(|width| {
        let overflow = toolbar.compute_overflow(*width as f32, false);
        overflow.tools_group_hidden() && !overflow.hidden_groups.contains(&ToolbarGroup::Dashboard)
    });
    assert!(
        dashboard_kept.is_some(),
        "应存在「主工具栏已收起、仪表盘仍在」的宽度区间"
    );
}

/// 左侧其它分组（循环/撤销/播放/录制）必须更晚折叠
#[test]
fn test_left_groups_collapse_last() {
    let toolbar = Toolbar::new();

    for width in WIDTHS {
        let overflow = toolbar.compute_overflow(width as f32, false);
        let deeper = [
            ToolbarGroup::Loop,
            ToolbarGroup::UndoRedo,
            ToolbarGroup::Playback,
            ToolbarGroup::Record,
        ]
        .iter()
        .any(|group| overflow.hidden_groups.contains(group));
        if deeper {
            assert!(
                overflow.tools_group_hidden(),
                "width={width}: 主工具栏必须先于左侧其它分组折叠"
            );
            assert!(
                overflow.hidden_groups.contains(&ToolbarGroup::Dashboard),
                "width={width}: 仪表盘必须先于左侧其它分组折叠"
            );
        }
    }
}

/// 工程走带模式：没有洋葱皮分组；工具区只能整组折叠（不做逐项）
#[test]
fn test_arrangement_mode_has_no_onion_and_whole_tools_group() {
    let toolbar = Toolbar::new();

    let wide = toolbar.compute_overflow(2400.0, true);
    assert!(!wide.has_overflow());
    assert!(
        wide.visible_groups.contains(&ToolbarGroup::Tools),
        "宽窗口下走带工具区应可见"
    );
    assert!(
        !wide.visible_groups.contains(&ToolbarGroup::OnionSkin),
        "走带模式没有洋葱皮分组"
    );
    assert!(wide.visible_tools.is_empty(), "走带模式不做逐项折叠");

    for width in WIDTHS {
        let overflow = toolbar.compute_overflow(width as f32, true);
        assert!(
            overflow.hidden_tools.is_empty(),
            "width={width}: 走带模式不应出现「部分工具」状态"
        );
    }
}

/// 「更多」按钮的尺寸契约：永远按原尺寸留在可视区内
#[test]
fn test_more_button_is_never_clipped() {
    let toolbar = Toolbar::new();
    assert_eq!(
        MORE_BUTTON_WIDTH, TOOL_BUTTON_WIDTH,
        "更多按钮宽度必须与普通工具按钮同源"
    );

    for width in WIDTHS {
        let available = width as f32;
        let content = (available - TOOLBAR_HORIZONTAL_PADDING).max(0.0);

        for arrangement_mode in [false, true] {
            let overflow = toolbar.compute_overflow(available, arrangement_mode);
            let layout = overflow.layout_width(arrangement_mode);
            assert!(
                layout <= content + 0.01,
                "width={width}（走带={arrangement_mode}）: 内容+更多按钮 {layout} 超出内容区 {content}，更多按钮会被裁"
            );
            if overflow.has_overflow() {
                assert!(
                    content >= MORE_GAP + MORE_BUTTON_WIDTH,
                    "width={width}: 内容区连「更多」按钮自身都放不下"
                );
            }
        }
    }
}

/// 折叠掉的工具按钮必须都能在「更多」菜单里找回（菜单不能是空面板）
#[test]
fn test_more_menu_covers_every_hidden_item() {
    let toolbar = Toolbar::new();

    for width in WIDTHS {
        let overflow = toolbar.compute_overflow(width as f32, false);
        if !overflow.has_overflow() {
            continue;
        }
        let items = toolbar.overflow_menu_items(&overflow, false, Language::ZhCn, false);
        assert!(
            !items.is_empty(),
            "width={width}: 显示了「更多」按钮却没有任何菜单项"
        );
        assert!(
            items.len() >= overflow.hidden_tools.len(),
            "width={width}: 折叠掉 {} 个工具按钮，菜单只有 {} 项",
            overflow.hidden_tools.len(),
            items.len()
        );
    }
}

/// 工具区"全展开宽度"保持历史标定 568（不因本轮改造漂移折叠触发点）
#[test]
fn test_tools_section_full_width_matches_baseline() {
    let full = tools_section_width(ToolItem::ALL, false);
    assert!(
        (full - 568.0).abs() < 0.5,
        "工具区全展开宽度应为 568（实际 {full}）"
    );
}
