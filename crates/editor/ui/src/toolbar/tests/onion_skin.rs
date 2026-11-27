//! 洋葱皮全局开关的工具栏行为测试（默认值 / 开合 / 分组可见性）
//!
//! 视图层（`view/onion_skin.rs`）只负责把点击翻成 `Event::ToggleOnionSkin`；
//! 可测语义全部收敛在 `Toolbar::update` 与宽度预算（`compute_overflow`）里，
//! 故测试落在这里。

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

    let piano_roll = toolbar.compute_overflow(4000.0, false);
    assert!(
        piano_roll.visible_groups.contains(&ToolbarGroup::OnionSkin),
        "钢琴卷帘工具栏应包含洋葱皮开关分组"
    );
    assert!(!piano_roll.has_overflow(), "宽度充足时不应折叠任何内容");

    let onion = piano_roll
        .visible_groups
        .iter()
        .position(|group| *group == ToolbarGroup::OnionSkin)
        .expect("可见分组里应有洋葱皮");
    let auto_scroll = piano_roll
        .visible_groups
        .iter()
        .position(|group| *group == ToolbarGroup::AutoScroll)
        .expect("可见分组里应有自动滚动");
    assert!(
        onion < auto_scroll,
        "洋葱皮开关应排在自动滚动左侧（同为显示开关，卷帘选项靠内）"
    );

    let arrangement = toolbar.compute_overflow(4000.0, true);
    assert!(
        !arrangement
            .visible_groups
            .contains(&ToolbarGroup::OnionSkin),
        "工程走带模式不应出现洋葱皮开关"
    );
    assert!(
        !arrangement.hidden_groups.contains(&ToolbarGroup::OnionSkin),
        "按模式排除的分组不得进入溢出菜单"
    );
}

/// 洋葱皮与自动滚动同为"显示开关"优先级，但 ORDER 在前：窄窗时洋葱皮先让位。
///
/// 用"最大仍被折叠的宽度"作判据，避免把某一条像素宽度写死在测试里
/// （宽度常量改动不该让行为测试变红，只有**顺序**变化才该变红）。
fn collapse_threshold(toolbar: &Toolbar, group: ToolbarGroup) -> Option<u32> {
    (320..=2400).rev().find(|width| {
        toolbar
            .compute_overflow(*width as f32, false)
            .hidden_groups
            .contains(&group)
    })
}

#[test]
fn test_onion_skin_collapses_before_auto_scroll() {
    let toolbar = Toolbar::new();

    let onion =
        collapse_threshold(&toolbar, ToolbarGroup::OnionSkin).expect("足够窄时洋葱皮开关应被折叠");
    let auto_scroll =
        collapse_threshold(&toolbar, ToolbarGroup::AutoScroll).expect("足够窄时自动滚动应被折叠");

    assert!(
        onion >= auto_scroll,
        "洋葱皮应先于自动滚动折叠（onion@{onion} 应不窄于 auto@{auto_scroll}）"
    );
}

// ── 显示范围下拉（2026-11-05）──

/// 默认范围必须是「全部音轨」：与没有模式字段前的老行为一致。
#[test]
fn test_onion_skin_mode_defaults_to_all_tracks() {
    let toolbar = Toolbar::new();
    assert_eq!(
        toolbar.onion_skin_mode,
        lumino_ui_core::onion_skin_mode::OnionSkinMode::AllTracks,
        "默认范围应为全部音轨"
    );
    assert!(!toolbar.onion_skin_menu_open, "下拉菜单默认关闭");
}

/// 三角开合：一次打开、再一次关闭；显式关闭事件也能关。
#[test]
fn test_onion_skin_menu_toggles() {
    let mut toolbar = Toolbar::new();

    toolbar.update(Event::ToggleOnionSkinMenu);
    assert!(toolbar.onion_skin_menu_open, "点三角应展开菜单");

    toolbar.update(Event::ToggleOnionSkinMenu);
    assert!(!toolbar.onion_skin_menu_open, "再点三角应收起菜单");

    toolbar.update(Event::ToggleOnionSkinMenu);
    toolbar.update(Event::CloseOnionSkinMenu);
    assert!(!toolbar.onion_skin_menu_open, "显式关闭事件应收起菜单");
}

/// 选中范围 = 写入模式 + **顺手打开开关** + 收起菜单。
///
/// 自动打开的理由：用户从下拉里选范围，就是"我要看这些轨的洋葱皮"；若开关还是
/// 关的，选完画面毫无变化，用户会当成功能坏了。
#[test]
fn test_selecting_mode_applies_and_enables_and_closes() {
    use lumino_ui_core::onion_skin_mode::OnionSkinMode;

    let mut toolbar = Toolbar::new();
    toolbar.onion_skin_menu_open = true;
    assert!(!toolbar.onion_skin_enabled, "前提：开关初始关闭");

    toolbar.update(Event::OnionSkinModeSelected(OnionSkinMode::PreviousTrack));

    assert_eq!(toolbar.onion_skin_mode, OnionSkinMode::PreviousTrack);
    assert!(toolbar.onion_skin_enabled, "选范围应顺手打开开关");
    assert!(!toolbar.onion_skin_menu_open, "选中后应收起菜单");
}

/// 「指定音轨」本轮未接入：**不得写入状态**（否则留下一个什么都没做的勾选态），
/// 同时菜单仍要收起（用户完成了一次点击）。提示由 root handler 负责。
#[test]
fn test_unimplemented_mode_is_not_written() {
    use lumino_ui_core::onion_skin_mode::OnionSkinMode;

    let mut toolbar = Toolbar::new();
    toolbar.onion_skin_mode = OnionSkinMode::NextTrack;
    toolbar.onion_skin_menu_open = true;

    toolbar.update(Event::OnionSkinModeSelected(OnionSkinMode::SpecificTrack));

    assert_eq!(
        toolbar.onion_skin_mode,
        OnionSkinMode::NextTrack,
        "未接入的模式不得覆盖当前模式"
    );
    assert!(!toolbar.onion_skin_menu_open, "点击后仍应收起菜单");
}

/// 关闭开关**不清模式**：重新打开时按原范围显示（用户硬约束：开关管显不显，模式管显哪些）。
#[test]
fn test_disabling_keeps_mode() {
    use lumino_ui_core::onion_skin_mode::OnionSkinMode;

    let mut toolbar = Toolbar::new();
    toolbar.update(Event::OnionSkinModeSelected(OnionSkinMode::PreviousTrack));
    toolbar.update(Event::ToggleOnionSkin);

    assert!(!toolbar.onion_skin_enabled);
    assert_eq!(
        toolbar.onion_skin_mode,
        OnionSkinMode::PreviousTrack,
        "关闭开关不得重置显示范围"
    );
}

/// 其它下拉/外部动作会把洋葱皮菜单收起（与画刷/形状下拉同一口径）。
#[test]
fn test_other_action_closes_onion_skin_menu() {
    let mut toolbar = Toolbar::new();
    toolbar.onion_skin_menu_open = true;

    toolbar.update(Event::ToolSelected(Tool::Pencil));

    assert!(!toolbar.onion_skin_menu_open, "外部动作应收起洋葱皮下拉");

    // 悬停事件不得关闭（重绘会重发 on_enter，会把刚打开的菜单关掉）
    toolbar.onion_skin_menu_open = true;
    toolbar.update(Event::ButtonHovered(Some(ButtonId::OnionSkin)));
    assert!(toolbar.onion_skin_menu_open, "悬停不应收起菜单");
}
