use super::*;

// ---------------------------------------------------------------------------
// 悬浮条条目的点击行为（普通点击 = 选择；**已启用条目再次点击 = 弹出设置**）
// 视图层的点击→消息映射见 `root/draw_toolbar.rs::tool_panel_item_press` 的专项测试。
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

/// 已启用条目再次点击（= `ToolPanelItemSettingsRequested`）：
/// 画刷 **并** 打开画刷设置下拉。
#[test]
fn test_tool_panel_settings_requested_brush_opens_dropdown() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;

    toolbar.update(Event::ToolPanelItemSettingsRequested(ToolPanelItem::Brush));
    assert_eq!(toolbar.current_tool, Tool::Brush, "再次点击画刷应切到画刷");
    assert!(
        toolbar.brush_dropdown_open,
        "再次点击画刷应打开画刷设置下拉"
    );
    assert!(!toolbar.shape_dropdown_open, "画刷/形状下拉应互斥");
}

/// 已启用条目再次点击：形状 **并** 打开形状选择下拉（矩形/圆形/三角形）。
#[test]
fn test_tool_panel_settings_requested_shape_opens_dropdown() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;

    toolbar.update(Event::ToolPanelItemSettingsRequested(ToolPanelItem::Shape));
    assert_eq!(toolbar.current_tool, Tool::Shape, "再次点击形状应切到形状");
    assert!(
        toolbar.shape_dropdown_open,
        "再次点击形状应打开形状选择下拉"
    );
    assert!(!toolbar.brush_dropdown_open, "画刷/形状下拉应互斥");
}

/// 已启用条目再次点击颜料桶：确保填充开启（分音符填充面板由 Root 侧弹出）。
/// 已开启状态下再次点击仍保持开启 —— 不因 toggle 语义被关掉
/// （关闭填充走面板内的「关闭填充」按钮）。
#[test]
fn test_tool_panel_settings_requested_fill_keeps_fill_on() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;
    toolbar.fill_enabled = false;

    toolbar.update(Event::ToolPanelItemSettingsRequested(
        ToolPanelItem::FillBucket,
    ));
    assert!(toolbar.fill_enabled, "再次点击颜料桶应开启填充");
    assert!(!toolbar.brush_dropdown_open && !toolbar.shape_dropdown_open);

    toolbar.update(Event::ToolPanelItemSettingsRequested(
        ToolPanelItem::FillBucket,
    ));
    assert!(toolbar.fill_enabled, "再次点击颜料桶不应把已开启的填充关掉");
}

/// 文字工具再次点击：打开文字参数面板，并关闭其它工具面板。
#[test]
fn test_text_settings_opens_text_params_panel() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;
    toolbar.brush_dropdown_open = true;
    toolbar.shape_dropdown_open = true;
    toolbar.thickness_panel_open = true;

    toolbar.update(Event::ToolPanelItemSettingsRequested(ToolPanelItem::Text));
    assert_eq!(toolbar.current_tool, Tool::Text);
    assert!(toolbar.text_params_open, "文字设置请求应打开参数面板");
    assert!(
        !toolbar.brush_dropdown_open
            && !toolbar.shape_dropdown_open
            && !toolbar.thickness_panel_open,
        "参数面板必须与其它工具面板互斥"
    );

    toolbar.update(Event::CloseTextParamsPanel);
    assert!(!toolbar.text_params_open, "关闭事件应收起参数面板");
}

/// 无独立设置的条目（曲线）收到设置请求：退化为普通选择，不打开任何下拉。
#[test]
fn test_tool_panel_settings_requested_curve_degrades_to_plain_select() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;
    toolbar.current_tool = Tool::Pencil;

    toolbar.update(Event::ToolPanelItemSettingsRequested(ToolPanelItem::Curve));
    assert_eq!(toolbar.current_tool, Tool::Curve);
    assert!(
        !toolbar.brush_dropdown_open && !toolbar.shape_dropdown_open,
        "无设置条目不应打开任何设置下拉"
    );
}

// ---------------------------------------------------------------------------
// 下拉开关语义
// ---------------------------------------------------------------------------

/// 画刷下拉打开时，面板内交互（只读展示 + 悬停）不应把它误关；
/// 仅点击面板内空白（`CloseBrushDropdown`）或再次切换（`ToggleBrushDropdown`）才关闭。
///
/// 注：此处原先钉的是"点画刷下拉里的粗细 +/- 按钮不关下拉"——那两个按钮已**删除**
/// （粗细是全局值，唯一控制入口是悬浮条右端的「粗细」面板滑杆），故改钉悬停/显式关闭。
#[test]
fn test_brush_dropdown_stays_open_on_hover_until_close_event() {
    let mut toolbar = Toolbar::new();
    toolbar.brush_dropdown_open = true;

    // 悬停事件不应关闭下拉（鼠标进出按钮会反复发它）
    toolbar.update(Event::ButtonHovered(None));
    assert!(toolbar.brush_dropdown_open, "悬停不应关闭画刷下拉");

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

/// 关闭悬浮工具条时应一并收起其承载的工具设置下拉与粗细面板。
///
/// 粗细面板锚定在悬浮条右端按钮上：悬浮条不在就没有锚点，留着"开"状态会让下次打开
/// 悬浮条时凭空弹出一块用户早已看不见的面板。
#[test]
fn test_close_tool_panel_closes_settings_dropdown() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;
    toolbar.brush_dropdown_open = true;
    toolbar.thickness_panel_open = true;

    toolbar.update(Event::ToggleToolPanel);
    assert!(!toolbar.tool_panel_open);
    assert!(
        !toolbar.brush_dropdown_open,
        "关闭悬浮条应一并收起其承载的画刷设置下拉"
    );
    assert!(
        !toolbar.thickness_panel_open,
        "关闭悬浮条应一并收起粗细面板（锚点已不存在）"
    );
}

// ---------------------------------------------------------------------------
// 「音符画设置」对话框请求（悬浮条右端齿轮按钮）
// ---------------------------------------------------------------------------

/// 齿轮请求**只作信道**：不改变工具语义。
///
/// 齿轮点开的是独立 OS 窗口，工具栏不是它的 owner（没有"开/关"状态位），
/// 故 `current_tool` / `fill_enabled` / 悬浮条位置一律不动——点设置不该顺手切走工具。
///
/// **但**窗口内临时浮层（画刷下拉等）照旧按"外部一动作即收起"的既有语义关闭，
/// 不为齿轮单开例外（统一比省一次点击重要）。
#[test]
fn test_open_draw_settings_dialog_leaves_tool_state_untouched() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;
    toolbar.current_tool = Tool::Brush;
    toolbar.fill_enabled = true;
    toolbar.brush_dropdown_open = true;
    let offset_before = toolbar.tool_panel_offset;

    toolbar.update(Event::OpenDrawSettingsDialog);

    assert_eq!(toolbar.current_tool, Tool::Brush, "点齿轮不应改变当前工具");
    assert!(toolbar.fill_enabled, "点齿轮不应关闭颜料桶");
    assert!(toolbar.tool_panel_open, "点齿轮不应关闭悬浮工具条");
    assert_eq!(
        toolbar.tool_panel_offset, offset_before,
        "点齿轮不应移动悬浮条"
    );
    assert!(
        !toolbar.brush_dropdown_open,
        "窗口内临时浮层照旧被'外部动作'收起（齿轮不享受例外）"
    );
}

/// 齿轮请求**不带开合态**：连点两次不产生任何"关"的语义。
///
/// 独立窗口由自己的标题栏负责开关；主窗按钮再维护一份状态只会与真实窗口状态漂移。
#[test]
fn test_open_draw_settings_dialog_is_idempotent() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;

    toolbar.update(Event::OpenDrawSettingsDialog);
    let after_first = (
        toolbar.current_tool,
        toolbar.fill_enabled,
        toolbar.tool_panel_open,
    );
    toolbar.update(Event::OpenDrawSettingsDialog);
    let after_second = (
        toolbar.current_tool,
        toolbar.fill_enabled,
        toolbar.tool_panel_open,
    );

    assert_eq!(
        after_first, after_second,
        "连续两次齿轮请求不应改变任何工具栏状态（无开合语义）"
    );
}

// ---------------------------------------------------------------------------
// 悬浮条右端「粗细」按钮 → 全局粗细面板（滑杆 1-100，步进 1）
// ---------------------------------------------------------------------------

/// 「粗细」按钮是**开合**语义：开 → 再点关；且开合**不动**工具状态。
///
/// 与齿轮（独立窗口、无开合态）的关键区别：本面板在主窗口内、锚定在悬浮条自己的
/// 按钮上，所以开合态必须由主窗维护；同时**不得**动 `current_tool` / `fill_enabled`
/// （它是设置入口，不是工具条目），也**不得**关掉悬浮条本身（否则锚点被一起拆掉）。
#[test]
fn test_thickness_panel_toggles_without_touching_tool_state() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;
    toolbar.current_tool = Tool::Shape;
    toolbar.fill_enabled = true;
    let offset_before = toolbar.tool_panel_offset;

    toolbar.update(Event::ToggleThicknessPanel);
    assert!(toolbar.thickness_panel_open, "首次点击「粗细」应打开面板");
    assert_eq!(toolbar.current_tool, Tool::Shape, "开面板不得切走当前工具");
    assert!(toolbar.fill_enabled, "开面板不得改动颜料桶填充态");
    assert!(
        toolbar.tool_panel_open,
        "开面板不得关闭悬浮工具条（锚点在它身上）"
    );
    assert_eq!(
        toolbar.tool_panel_offset, offset_before,
        "开面板不得移动悬浮条"
    );

    toolbar.update(Event::ToggleThicknessPanel);
    assert!(
        !toolbar.thickness_panel_open,
        "再次点击「粗细」应收起面板（开合语义）"
    );
}

/// 面板打开时，**滑杆**的粗细变更必须放行且不关面板——否则滑杆一拖就把自己关掉。
#[test]
fn test_thickness_panel_stays_open_while_dragging_slider() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;
    toolbar.update(Event::ToggleThicknessPanel);
    assert!(toolbar.thickness_panel_open, "前置：面板已打开");

    for v in [5_u8, 40, 100, 1] {
        toolbar.update(Event::DrawThicknessChanged(v));
        assert!(
            toolbar.thickness_panel_open,
            "拖滑杆（粗细={v}）不得关闭面板"
        );
        assert_eq!(toolbar.brush.thickness, v, "粗细应写入全局值");
    }

    // 显式关闭（点面板内空白）
    toolbar.update(Event::CloseThicknessPanel);
    assert!(!toolbar.thickness_panel_open, "关闭事件应收起面板");
}

/// 全局粗细与滑杆量程同源：越界值由 `set_thickness` 夹到 `1..=100`。
///
/// 这条同时钉死"上限从 20 放宽到 100"：滑杆右端 100 必须能原样存下来，
/// 而不是被静默截成 20（那正是"滑到头却没变化"的一类静默失效）。
#[test]
fn test_draw_thickness_changed_clamps_to_slider_range() {
    let mut toolbar = Toolbar::new();

    toolbar.update(Event::DrawThicknessChanged(0));
    assert_eq!(
        toolbar.brush.thickness,
        BrushConfig::MIN_THICKNESS,
        "越界下限应夹到最小粗细"
    );

    toolbar.update(Event::DrawThicknessChanged(200));
    assert_eq!(
        toolbar.brush.thickness,
        BrushConfig::MAX_THICKNESS,
        "越界上限应夹到最大粗细（= 滑杆右端）"
    );
    assert_eq!(BrushConfig::MAX_THICKNESS, 100, "滑杆右端应为 100");
}

/// 面板与其余**窗口内**面板互斥；且任何"外部动作"都会收起它（不给新入口开例外）。
#[test]
fn test_thickness_panel_exclusive_and_closed_by_external_actions() {
    let mut toolbar = Toolbar::new();
    toolbar.tool_panel_open = true;

    // 打开粗细面板 → 收起画刷 / 形状下拉
    toolbar.brush_dropdown_open = true;
    toolbar.shape_dropdown_open = true;
    toolbar.update(Event::ToggleThicknessPanel);
    assert!(toolbar.thickness_panel_open);
    assert!(
        !toolbar.brush_dropdown_open && !toolbar.shape_dropdown_open,
        "打开粗细面板应收起其余窗口内面板"
    );

    // 反向：打开画刷设置（工具条目再次点击）应收起粗细面板
    toolbar.update(Event::ToolPanelItemSettingsRequested(ToolPanelItem::Brush));
    assert!(
        !toolbar.thickness_panel_open,
        "其它工具栏动作（工具条目再次点击）应收起粗细面板"
    );

    // 外部动作同样收起（此处用齿轮的独立窗口请求做代表）
    toolbar.update(Event::ToggleThicknessPanel);
    assert!(toolbar.thickness_panel_open);
    toolbar.update(Event::OpenDrawSettingsDialog);
    assert!(
        !toolbar.thickness_panel_open,
        "齿轮请求照旧按'外部一动作即收起'处理（新面板不享受例外）"
    );
}
