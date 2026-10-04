use super::*;

// ---------------------------------------------------------------------------
// 悬浮条条目的「普通点击 / Ctrl+点击」行为（原主工具栏入口按钮 Ctrl 旁路已整体迁到悬浮条）
// ---------------------------------------------------------------------------

/// 悬浮条普通点击条目：仅选择工具，不打开任何设置下拉。
#[test]
fn test_tool_panel_plain_click_selects_without_opening_settings() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;

    toolbar.update(Event::ToolPanelItemSelected(ToolPanelItem::Brush));
    assert_eq!(toolbar.current_tool, Tool::Brush, "普通点击应选择画刷工具");
    assert!(!toolbar.brush_dropdown_open, "普通点击不应打开画刷设置下拉");
    assert!(toolbar.tool_panel_open, "选择条目后悬浮条应保持打开");
}

/// Ctrl+点击画刷：选择画刷 **并** 打开画刷设置下拉。
#[test]
fn test_tool_panel_ctrl_click_brush_selects_and_opens_dropdown() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;

    toolbar.update(Event::ToolPanelItemCtrlSelected(ToolPanelItem::Brush));
    assert_eq!(toolbar.current_tool, Tool::Brush, "Ctrl+点击画刷应切到画刷");
    assert!(
        toolbar.brush_dropdown_open,
        "Ctrl+点击画刷应打开画刷设置下拉"
    );
    assert!(!toolbar.shape_dropdown_open, "画刷/形状下拉应互斥");
}

/// Ctrl+点击形状：选择形状 **并** 打开形状选择下拉（矩形/圆形/三角形）。
#[test]
fn test_tool_panel_ctrl_click_shape_selects_and_opens_dropdown() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;

    toolbar.update(Event::ToolPanelItemCtrlSelected(ToolPanelItem::Shape));
    assert_eq!(toolbar.current_tool, Tool::Shape, "Ctrl+点击形状应切到形状");
    assert!(
        toolbar.shape_dropdown_open,
        "Ctrl+点击形状应打开形状选择下拉"
    );
    assert!(!toolbar.brush_dropdown_open, "画刷/形状下拉应互斥");
}

/// Ctrl+点击颜料桶：确保填充开启（分音符填充对话框由 Root 侧弹出）。
/// 已开启状态下再 Ctrl+点击仍保持开启 —— 不因 toggle 语义被关掉。
#[test]
fn test_tool_panel_ctrl_click_fill_enables_fill() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;
    toolbar.fill_enabled = false;

    toolbar.update(Event::ToolPanelItemCtrlSelected(ToolPanelItem::FillBucket));
    assert!(toolbar.fill_enabled, "Ctrl+点击颜料桶应开启填充");
    assert!(!toolbar.brush_dropdown_open && !toolbar.shape_dropdown_open);

    toolbar.update(Event::ToolPanelItemCtrlSelected(ToolPanelItem::FillBucket));
    assert!(
        toolbar.fill_enabled,
        "Ctrl+点击颜料桶不应把已开启的填充关掉"
    );
}

/// Ctrl+点击无独立设置的条目（曲线）：退化为普通选择，不打开任何下拉。
#[test]
fn test_tool_panel_ctrl_click_curve_degrades_to_plain_select() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;
    toolbar.current_tool = Tool::Pencil;

    toolbar.update(Event::ToolPanelItemCtrlSelected(ToolPanelItem::Curve));
    assert_eq!(toolbar.current_tool, Tool::Curve);
    assert!(
        !toolbar.brush_dropdown_open && !toolbar.shape_dropdown_open,
        "无设置条目 Ctrl+点击不应打开任何设置下拉"
    );
}

// ---------------------------------------------------------------------------
// 下拉开关语义
// ---------------------------------------------------------------------------

/// 画刷下拉内部的粗细 +/- 按钮点击后，下拉应保持打开（不被误关）。
/// 仅点击面板内空白（CloseBrushDropdown）或再次切换（ToggleBrushDropdown）才关闭。
#[test]
fn test_brush_dropdown_stays_open_on_thickness_button() {
    let mut toolbar = Toolbar::new();
    toolbar.brush_dropdown_open = true;

    // 点击下拉内部的「+」按钮：改粗细，但不应关闭下拉
    toolbar.update(Event::BrushThicknessChanged(5));
    assert!(
        toolbar.brush_dropdown_open,
        "点击画刷下拉内部的粗细按钮不应关闭下拉"
    );

    // 再次点击「-」按钮：同样保持打开（连续操作不关闭）
    toolbar.update(Event::BrushThicknessChanged(3));
    assert!(
        toolbar.brush_dropdown_open,
        "连续操作画刷下拉按钮不应关闭下拉"
    );

    // 点击面板内空白（外部关闭消息）：应关闭
    toolbar.update(Event::CloseBrushDropdown);
    assert!(
        !toolbar.brush_dropdown_open,
        "CloseBrushDropdown（点击面板外空白）应关闭画刷下拉"
    );
}

/// 画刷下拉打开时，再次点击附属按钮（ToggleBrushDropdown）应切换关闭。
#[test]
fn test_brush_dropdown_toggle_closes() {
    let mut toolbar = Toolbar::new();
    toolbar.brush_dropdown_open = true;
    toolbar.update(Event::ToggleBrushDropdown);
    assert!(
        !toolbar.brush_dropdown_open,
        "打开状态下再次 ToggleBrushDropdown 应关闭画刷下拉"
    );
}

/// 选择某个图形类型（ShapeTypeSelected）应写入 current_shape 状态变量，
/// 并自动关闭形状工具下拉（视为一次选择完成）。
#[test]
fn test_shape_type_selected_updates_state_and_closes_dropdown() {
    let mut toolbar = Toolbar::new();
    toolbar.current_tool = Tool::Shape;
    toolbar.shape_dropdown_open = true;
    assert_eq!(toolbar.current_shape, ShapeType::Rectangle);

    toolbar.update(Event::ShapeTypeSelected(ShapeType::Circle));
    assert_eq!(
        toolbar.current_shape,
        ShapeType::Circle,
        "ShapeTypeSelected 应更新 current_shape 状态变量"
    );
    assert!(
        !toolbar.shape_dropdown_open,
        "选择图形后形状工具下拉应自动关闭"
    );

    // 再切到三角形，验证状态变量可被正确更新多次
    toolbar.shape_dropdown_open = true;
    toolbar.update(Event::ShapeTypeSelected(ShapeType::Triangle));
    assert_eq!(toolbar.current_shape, ShapeType::Triangle);
}

/// 切换工具（ToolSelected）应关闭所有可能残留的下拉，包括形状工具下拉。
#[test]
fn test_tool_selected_closes_shape_dropdown() {
    let mut toolbar = Toolbar::new();
    toolbar.shape_dropdown_open = true;
    toolbar.current_shape = ShapeType::Circle;

    toolbar.update(Event::ToolSelected(Tool::Pencil));
    assert!(!toolbar.shape_dropdown_open, "切换工具应关闭形状工具下拉");
    // current_shape 作为持久偏好保留，不应被重置
    assert_eq!(toolbar.current_shape, ShapeType::Circle);
}

// ---------------------------------------------------------------------------
// 拖拽状态机 + 自动吸附
// ---------------------------------------------------------------------------

/// 悬浮工具条拖拽状态机：起拖 → 逐帧增量位移 → 松手结束。
/// 对应根因修复 —— 拖拽柄（独立于按钮区）按下必达起拖，随后由全窗口覆盖层逐帧递推。
#[test]
fn test_tool_panel_drag_accumulates_offset() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;
    let (dx0, dy0) = toolbar.tool_panel_offset; // 默认 (0, 44)

    // 起拖：进入拖拽态（拖拽柄 on_press 发出）
    toolbar.update(Event::ToolPanelDragStarted);
    assert!(toolbar.tool_panel_dragging, "起拖后应进入拖拽态");

    // 首个 move：仅记录基准点，尚未产生位移
    toolbar.update(Event::ToolPanelDragged(100.0, 100.0));
    assert!(
        (toolbar.tool_panel_offset.0 - dx0).abs() < 1e-3
            && (toolbar.tool_panel_offset.1 - dy0).abs() < 1e-3,
        "首帧仅记录基准点，不应位移"
    );

    // 第二个 move：向右 30、向上 20 → dx +30、dy +20（向上 = 距底内缩增大）
    toolbar.update(Event::ToolPanelDragged(130.0, 80.0));
    assert!(
        (toolbar.tool_panel_offset.0 - (dx0 + 30.0)).abs() < 1e-3
            && (toolbar.tool_panel_offset.1 - (dy0 + 20.0)).abs() < 1e-3,
        "拖拽应向右(+dx)/向上(+dy) 增量位移，实际 {:?}",
        toolbar.tool_panel_offset
    );

    // 松手：结束拖拽态（偏移 (30,64) 已超吸附阈值，保留原位）
    toolbar.update(Event::ToolPanelDragEnded);
    assert!(!toolbar.tool_panel_dragging, "松手后应退出拖拽态");
    assert_eq!(
        toolbar.tool_panel_offset,
        (30.0, 64.0),
        "远离默认位的释放点应保留原位"
    );
}

/// 未起拖时的 move 不产生位移（避免悬停/其它来源的 move 误移动面板）。
#[test]
fn test_tool_panel_drag_ignored_without_start() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;
    let before = toolbar.tool_panel_offset;
    toolbar.update(Event::ToolPanelDragged(500.0, 100.0));
    assert!(
        (toolbar.tool_panel_offset.0 - before.0).abs() < 1e-3
            && (toolbar.tool_panel_offset.1 - before.1).abs() < 1e-3,
        "未起拖的 move 不应位移"
    );
}

/// 拖拽结束吸附：释放点接近默认位（两轴均在阈值内）时吸回默认位。
#[test]
fn test_drag_release_snaps_to_default_when_near() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;
    // 拖到默认位附近：dx=10、dy=60（两轴均在 28px 阈值内）
    toolbar.tool_panel_offset = (10.0, 60.0);

    toolbar.update(Event::ToolPanelDragEnded);
    assert_eq!(
        toolbar.tool_panel_offset, TOOL_PANEL_DEFAULT_OFFSET,
        "接近默认位的释放点应吸附回默认位"
    );
}

/// 拖拽结束吸附：释放点远离默认位时保留原位（允许停靠到任意位置）。
#[test]
fn test_drag_release_keeps_position_when_far() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;
    toolbar.tool_panel_offset = (200.0, 300.0);

    toolbar.update(Event::ToolPanelDragEnded);
    assert_eq!(
        toolbar.tool_panel_offset,
        (200.0, 300.0),
        "远离默认位的释放点不应被吸附"
    );
}

/// 拖拽结束吸附：仅单轴接近（另一轴远离）时不吸附 —— 须两轴都接近。
#[test]
fn test_drag_release_snap_requires_both_axes_near() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;
    toolbar.tool_panel_offset = (5.0, 200.0); // dx 近、dy 远

    toolbar.update(Event::ToolPanelDragEnded);
    assert_eq!(
        toolbar.tool_panel_offset,
        (5.0, 200.0),
        "只有单轴接近时不应吸附"
    );
}

/// 关闭悬浮工具条时应复位拖拽态与抓取点，避免残留的全窗口覆盖层拦截后续交互。
#[test]
fn test_toggle_close_resets_drag_state() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;
    toolbar.update(Event::ToolPanelDragStarted);
    toolbar.tool_panel_last_cursor = Some((10.0, 10.0));

    toolbar.update(Event::ToggleToolPanel);
    assert!(!toolbar.tool_panel_open, "再次 ToggleToolPanel 应关闭");
    assert!(!toolbar.tool_panel_dragging, "关闭后应复位拖拽态");
    assert!(
        toolbar.tool_panel_last_cursor.is_none(),
        "关闭后应清空抓取点"
    );
}

/// 关闭悬浮工具条时应一并收起其承载的工具设置下拉。
#[test]
fn test_close_tool_panel_closes_settings_dropdown() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;
    toolbar.brush_dropdown_open = true;

    toolbar.update(Event::ToggleToolPanel);
    assert!(!toolbar.tool_panel_open);
    assert!(
        !toolbar.brush_dropdown_open,
        "关闭悬浮条应一并收起其承载的画刷设置下拉"
    );
}
