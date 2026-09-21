use super::*;

/// 非画刷工具下 Ctrl+点击曲线按钮：不得弹出画刷设置面板，
/// 应退化为普通点击（选择曲线工具）。
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
        matches!(event, Event::ToolSelected(Tool::Curve)),
        "非画刷工具下 Ctrl+点击应退化为选择曲线工具"
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

/// 画刷工具下但 Ctrl 未按下：普通点击选择曲线工具，不弹面板。
#[test]
fn test_curve_button_normal_click_brush_tool_does_not_open_brush_panel() {
    let mut toolbar = Toolbar::new();
    toolbar.current_tool = Tool::Brush;
    toolbar.ctrl_pressed = false;

    let event = toolbar.curve_button_press_event();
    assert!(
        matches!(event, Event::ToolSelected(Tool::Curve)),
        "画刷工具下普通点击应回到曲线工具"
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

/// 形状工具下但 Ctrl 未按下：普通点击应退化为选择曲线工具，不弹菜单。
#[test]
fn test_shape_button_normal_click_shape_tool_does_not_open_shape_dropdown() {
    let mut toolbar = Toolbar::new();
    toolbar.current_tool = Tool::Shape;
    toolbar.ctrl_pressed = false;

    let event = toolbar.curve_button_press_event();
    assert!(
        matches!(event, Event::ToolSelected(Tool::Curve)),
        "形状工具下普通点击应回到曲线工具"
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
