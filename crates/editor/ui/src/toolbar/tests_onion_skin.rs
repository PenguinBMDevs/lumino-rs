//! 洋葱皮全局开关的工具栏行为测试（默认值 / 开合 / 分组可见性）
//!
//! 视图层（`view/onion_skin.rs`）只负责把点击翻成 `Event::ToggleOnionSkin`；
//! 可测语义全部收敛在 `Toolbar::update` 与溢出分组计算里，故测试落在这里。

use super::*;
use crate::toolbar::overflow::ToolbarGroup;

/// 默认关闭：新工具栏不得默认显示其它音轨的幽灵音符。
#[test]
fn test_onion_skin_defaults_to_off() {
    let toolbar = Toolbar::new();
    assert!(!toolbar.onion_skin_enabled, "洋葱皮默认应为关闭");
}

/// 点一次 = 开，再点一次 = 关（纯开/关，不夹带第三种状态）。
#[test]
fn test_toggle_onion_skin_flips_state() {
    let mut toolbar = Toolbar::new();

    toolbar.update(Event::ToggleOnionSkin);
    assert!(toolbar.onion_skin_enabled, "首次点击应打开洋葱皮");

    toolbar.update(Event::ToggleOnionSkin);
    assert!(!toolbar.onion_skin_enabled, "再次点击应关闭洋葱皮");
}

/// 它是**纯显示开关**：不得改动当前工具与颜料桶填充态；常驻悬浮条也不受影响。
///
/// 窗口内下拉（画刷等）照旧"外部一动作即收起"——那是工具栏统一口径，
/// 新按钮不享受例外（否则同一动作下各浮层行为不一致）。
#[test]
fn test_toggle_onion_skin_leaves_tool_state_untouched() {
    let mut toolbar = Toolbar::new();
    toolbar.current_tool = Tool::Pencil;
    toolbar.fill_enabled = true;
    toolbar.tool_panel_open = true;
    toolbar.brush_dropdown_open = true;

    toolbar.update(Event::ToggleOnionSkin);

    assert!(toolbar.onion_skin_enabled, "前提：开关已打开");
    assert_eq!(toolbar.current_tool, Tool::Pencil, "不得切走当前工具");
    assert!(toolbar.fill_enabled, "不得改动颜料桶填充态");
    assert!(
        toolbar.tool_panel_open,
        "音符画悬浮工具条是常驻浮层，不随显示开关收起"
    );
    assert!(
        !toolbar.brush_dropdown_open,
        "窗口内下拉照旧按外部动作收起（统一口径）"
    );
}

/// 分组可见性：钢琴卷帘显示该开关；工程走带（没有洋葱皮层）整组不出现，
/// 也不落进溢出菜单——否则会留下一个点了没任何效果的菜单项。
#[test]
fn test_onion_skin_group_visible_only_in_piano_roll() {
    let toolbar = Toolbar::new();

    let (visible, hidden) = toolbar.compute_overflow_groups(4000.0, false);
    assert!(
        visible.contains(&ToolbarGroup::OnionSkin),
        "钢琴卷帘工具栏应包含洋葱皮开关分组"
    );
    assert!(hidden.is_empty(), "宽度充足时不应折叠任何分组");

    let onion = visible
        .iter()
        .position(|group| *group == ToolbarGroup::OnionSkin)
        .expect("可见分组里应有洋葱皮");
    let auto_scroll = visible
        .iter()
        .position(|group| *group == ToolbarGroup::AutoScroll)
        .expect("可见分组里应有自动滚动");
    assert!(
        onion < auto_scroll,
        "洋葱皮开关应排在自动滚动左侧（同为显示开关，卷帘选项靠内）"
    );

    let (visible_arrangement, hidden_arrangement) = toolbar.compute_overflow_groups(4000.0, true);
    assert!(
        !visible_arrangement.contains(&ToolbarGroup::OnionSkin),
        "工程走带模式不应出现洋葱皮开关"
    );
    assert!(
        hidden_arrangement.is_empty(),
        "按模式排除的分组不得进入溢出菜单"
    );
}

/// 宽度不足时洋葱皮先被折叠：与自动滚动同为"显示开关"优先级，但 ORDER 在前，
/// 稳定排序决定它先让位（新功能不抢既有按钮的常驻位置）。
#[test]
fn test_onion_skin_collapses_before_auto_scroll() {
    let toolbar = Toolbar::new();

    let total: f32 = ToolbarGroup::ORDER
        .iter()
        .map(|group| group.width() + group.spacing_after())
        .sum();
    // 恰好容得下"除洋葱皮以外"的全部分组
    let available =
        total - (ToolbarGroup::OnionSkin.width() + ToolbarGroup::OnionSkin.spacing_after());

    let (visible, hidden) = toolbar.compute_overflow_groups(available, false);
    assert!(
        hidden.contains(&ToolbarGroup::OnionSkin),
        "宽度不足时洋葱皮应先被折叠到溢出菜单"
    );
    assert!(
        !hidden.contains(&ToolbarGroup::AutoScroll),
        "自动滚动与洋葱皮同优先级但 ORDER 靠后，应晚于洋葱皮折叠"
    );
    assert!(!visible.contains(&ToolbarGroup::OnionSkin));
    assert!(visible.contains(&ToolbarGroup::AutoScroll));
}
