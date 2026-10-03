use super::*;

/// 非画刷工具下 Ctrl+点击绘制入口按钮：不得弹出画刷设置面板，
/// 应退化为普通点击（开关音符绘制悬浮工具条）。
#[test]
fn test_curve_button_ctrl_click_non_brush_does_not_open_brush_panel() {
    let mut toolbar = Toolbar::new();
    // 当前处于选择工具（非画刷），且 Ctrl 被按下
    toolbar.current_tool = Tool::Pointer;
    toolbar.ctrl_pressed = true;

    let event = toolbar.curve_button_press_event();
    assert!(
        !matches!(event, Event::ToggleBrushDropdown),
        "非画刷工具下 Ctrl+点击不应打开画刷设置面板"
    );
    assert!(
        matches!(event, Event::ToggleToolPanel),
        "非画刷工具下 Ctrl+点击应退化为开关绘制悬浮工具条"
    );
}

/// 曲线工具（非画刷）下 Ctrl+点击曲线按钮：同样不应弹出画刷设置面板。
#[test]
fn test_curve_button_ctrl_click_curve_tool_does_not_open_brush_panel() {
    let mut toolbar = Toolbar::new();
    toolbar.current_tool = Tool::Curve;
    toolbar.ctrl_pressed = true;

    let event = toolbar.curve_button_press_event();
    assert!(
        !matches!(event, Event::ToggleBrushDropdown),
        "曲线工具下 Ctrl+点击不应打开画刷设置面板"
    );
}

/// 仅当已处于画刷工具且 Ctrl 按下时，才打开画刷设置面板。
#[test]
fn test_curve_button_ctrl_click_brush_tool_opens_brush_panel() {
    let mut toolbar = Toolbar::new();
    toolbar.current_tool = Tool::Brush;
    toolbar.ctrl_pressed = true;

    let event = toolbar.curve_button_press_event();
    assert!(
        matches!(event, Event::ToggleBrushDropdown),
        "画刷工具下 Ctrl+点击应打开画刷设置面板"
    );
}

/// 油漆桶开启时（Curve+fill）Ctrl+点击曲线按钮：应请求打开「分音符填充」对话框，
/// 不得退化为 ToolSelected(Curve) —— 那会把油漆桶打回曲线（图标回退 bug 根因）。
#[test]
fn test_curve_button_ctrl_click_with_fill_opens_fill_division_dialog() {
    let mut toolbar = Toolbar::new();
    toolbar.current_tool = Tool::Curve;
    toolbar.ctrl_pressed = true;
    toolbar.fill_enabled = true;

    let event = toolbar.curve_button_press_event();
    assert!(
        matches!(event, Event::OpenFillDivisionDialog),
        "油漆桶开启时 Ctrl+点击应打开分音符填充对话框: {event:?}"
    );
}

/// 油漆桶未开启时（Curve 无 fill）Ctrl+点击绘制入口按钮：
/// 无上下文时退化为开关绘制悬浮工具条。
#[test]
fn test_curve_button_ctrl_click_without_fill_toggles_tool_panel() {
    let mut toolbar = Toolbar::new();
    toolbar.current_tool = Tool::Curve;
    toolbar.ctrl_pressed = true;
    toolbar.fill_enabled = false;

    let event = toolbar.curve_button_press_event();
    assert!(
        matches!(event, Event::ToggleToolPanel),
        "油漆桶未开启时 Ctrl+点击应开关绘制悬浮工具条: {event:?}"
    );
}

/// 画刷工具下但 Ctrl 未按下：普通点击开关绘制悬浮工具条，不弹画刷面板。
#[test]
fn test_curve_button_normal_click_brush_tool_does_not_open_brush_panel() {
    let mut toolbar = Toolbar::new();
    toolbar.current_tool = Tool::Brush;
    toolbar.ctrl_pressed = false;

    let event = toolbar.curve_button_press_event();
    assert!(
        matches!(event, Event::ToggleToolPanel),
        "画刷工具下普通点击应开关绘制悬浮工具条"
    );
}

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

/// 形状工具激活且 Ctrl 按下时，点击曲线工具组按钮应打开形状工具下拉
/// （矩形/圆形/三角形选择菜单），而非退化为选择曲线工具。
#[test]
fn test_shape_button_ctrl_click_shape_tool_opens_shape_dropdown() {
    let mut toolbar = Toolbar::new();
    toolbar.current_tool = Tool::Shape;
    toolbar.ctrl_pressed = true;

    let event = toolbar.curve_button_press_event();
    assert!(
        matches!(event, Event::ToggleShapeDropdown),
        "形状工具下 Ctrl+点击应打开形状工具下拉"
    );

    toolbar.update(event);
    assert!(
        toolbar.shape_dropdown_open,
        "ToggleShapeDropdown 应打开形状工具下拉"
    );
    // 打开形状下拉时应关闭其它浮层（互斥）
    assert!(!toolbar.brush_dropdown_open && !toolbar.tool_panel_open);
}

/// 形状工具下但 Ctrl 未按下：普通点击应开关绘制悬浮工具条，不弹图形菜单。
#[test]
fn test_shape_button_normal_click_shape_tool_does_not_open_shape_dropdown() {
    let mut toolbar = Toolbar::new();
    toolbar.current_tool = Tool::Shape;
    toolbar.ctrl_pressed = false;

    let event = toolbar.curve_button_press_event();
    assert!(
        matches!(event, Event::ToggleToolPanel),
        "形状工具下普通点击应开关绘制悬浮工具条"
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

    // 松手：结束拖拽态
    toolbar.update(Event::ToolPanelDragEnded);
    assert!(!toolbar.tool_panel_dragging, "松手后应退出拖拽态");
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
