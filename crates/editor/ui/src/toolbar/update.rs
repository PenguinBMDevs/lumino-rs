//! 工具栏事件分发：互斥浮层收起守卫 + 按事件族分派。
//!
//! 具体处理拆到 `update_*` 子模块；本文件只保留 `new`、守卫、穷尽式分派表、
//! 尺寸拖拽与仅日志事件。分派表分两级：`classify_event`（事件 → 事件族）与
//! `dispatch_event`（事件族 → 处理函数）；两级都不写 `_` 兜底，编译器会强制穷尽，
//! 新增事件漏归类会编译失败，避免被静默吞掉。

use super::*;

/// 本事件允许保持打开的浮层。
#[derive(Clone, Copy)]
enum PopoverKeep {
    /// 悬停：不收起任何浮层（重绘会让 mouse_area 重发 on_enter）。
    All,
    Only(Popover),
    None,
}

/// 事件族：分派表的第一级，把「事件 → 处理函数」一次映射拆成
/// 「事件 → 事件族」（`classify_event`）与「事件族 → 处理函数」（`dispatch_event`）。
///
/// 两级都是穷尽匹配，都不写 `_` 兜底：新增事件漏归类会在 `classify_event` 处编译失败。
enum EventFamily {
    /// 走带与录制
    Transport,
    /// 工具选择与绘制工具条
    Tools,
    /// 绘制工具条拖拽
    ToolPanelDrag,
    /// 吸附规格与吸附选择器
    Snap,
    /// 工具栏下拉（溢出 / 画刷 / 形状）
    Dropdowns,
    /// 粗细面板
    Thickness,
    /// 自定义精度对话框
    CustomPrecision,
    /// 仪表盘编辑（BPM / PPQ）与自动滚动
    Dashboard,
    /// 工具栏高度拖拽
    Resize,
    /// 仅写日志 / 无状态可改的事件
    LogOnly,
    /// 洋葱皮事件（`update` 已提前返回）与悬停（由 Root 写入状态栏）：
    /// 工具栏都不改状态，只留空族让分派表穷尽。
    Ignored,
}

impl Toolbar {
    /// 创建新的工具栏
    pub fn new() -> Self {
        Self {
            current_tool: Tool::default(),
            is_playing: false,
            is_looping: false,
            is_recording: false,
            height: DEFAULT_HEIGHT,
            is_resizing: false,
            resize_start_y: 0.0,
            resize_start_height: DEFAULT_HEIGHT,
            snap_spec: SnapSpec::QUARTER,
            snap_picker_open: false,
            speed_factor: 0.5,
            ctrl_pressed: false,
            shift_pressed: false,
            custom_precision_dialog: CustomPrecisionDialog::default(),
            auto_scroll_mode: lumino_core::storage::config::AutoScrollMode::default(),
            // 洋葱皮默认**关闭**：其它音轨的幽灵音符不显示，需用户显式打开
            onion_skin_enabled: false,
            // 显示范围默认「全部音轨」：与删除模式字段前的老行为一致
            onion_skin_mode: lumino_ui_core::onion_skin_mode::OnionSkinMode::default(),
            active_popover: None,
            ppq_editing: false,
            ppq_edit_buffer: String::new(),
            ppq_edit_error: None,
            bpm_editing: false,
            bpm_edit_buffer: String::new(),
            bpm_edit_error: None,
            bpm_edit_anchor_tick: 0.0,
            tool_panel_open: false,
            // 默认位置：水平居中（dx=0）、距窗口底部内缩 44px（悬浮于卷帘区域下缘）
            tool_panel_offset: TOOL_PANEL_DEFAULT_OFFSET,
            tool_panel_dragging: false,
            tool_panel_last_cursor: None,
            brush: BrushConfig::new(),
            fill_enabled: false,
            arc_mode: false,
            current_shape: ShapeType::default(),
        }
    }

    /// 更新工具栏状态
    pub fn update(&mut self, event: Event) {
        self.dismiss_popovers(&event);

        // 洋葱皮事件（开关 / 下拉开合 / 选择范围）集中到 `update_onion_skin.rs`，
        // 命中即返回；外部动作收起下拉的守卫已在上面统一跑过。
        if self.update_onion_skin_event(&event) {
            return;
        }

        self.dispatch_event(event);
    }

    /// 按事件族分派到具体处理函数；`EventFamily` 上没有 `_` 兜底，漏接新族会编译失败。
    fn dispatch_event(&mut self, event: Event) {
        match Self::classify_event(&event) {
            EventFamily::Transport => self.update_transport(event),
            EventFamily::Tools => self.update_tools(event),
            EventFamily::ToolPanelDrag => self.update_tool_panel_drag(event),
            EventFamily::Snap => self.update_snap(event),
            EventFamily::Dropdowns => self.update_dropdowns(event),
            EventFamily::Thickness => self.update_thickness(event),
            EventFamily::CustomPrecision => self.update_custom_precision(event),
            EventFamily::Dashboard => self.update_dashboard(event),
            EventFamily::Resize => self.update_resize(event),
            EventFamily::LogOnly => self.log_unhandled(event),
            // 洋葱皮事件已由 `update` 前面的 `update_onion_skin_event` 提前返回，
            // 悬停描述由 Root 写入状态栏：两者都只留空臂。
            EventFamily::Ignored => {}
        }
    }

    /// 把事件归入所属事件族；穷尽匹配，不写 `_` 兜底。
    ///
    /// 与 `dispatch_event` 是同一张分派表的两级，故放在同一个 `impl` 内并排阅读。
    fn classify_event(event: &Event) -> EventFamily {
        use EventFamily::*;
        match event {
            // 走带与录制
            Event::Play
            | Event::Pause
            | Event::Stop
            | Event::SkipBackward
            | Event::SkipForward
            | Event::SkipBackwardToStart
            | Event::SkipForwardToEnd
            | Event::ToggleLoop
            | Event::Record
            | Event::RecordStop => Transport,

            // 工具选择与绘制工具条
            Event::ToolSelected(_)
            | Event::FillToggled(_)
            | Event::ToggleToolPanel
            | Event::CloseToolPanel
            | Event::ToolPanelItemSelected(_)
            | Event::ToolPanelItemSettingsRequested(_)
            | Event::CloseTextParamsPanel => Tools,

            // 绘制工具条拖拽
            Event::ToolPanelDragStarted
            | Event::ToolPanelDragEnded
            | Event::ToolPanelDragged(_, _) => ToolPanelDrag,

            // 吸附规格与吸附选择器
            Event::SnapSpecChanged(_)
            | Event::PrecisionChanged(_)
            | Event::ToggleSnapPicker
            | Event::CloseSnapPicker => Snap,

            // 工具栏下拉（溢出 / 画刷 / 形状）
            Event::ToggleOverflowMenu
            | Event::CloseOverflowMenu
            | Event::ToggleBrushDropdown
            | Event::CloseBrushDropdown
            | Event::ToggleShapeDropdown
            | Event::CloseShapeDropdown
            | Event::ShapeTypeSelected(_) => Dropdowns,

            // 粗细面板
            Event::ToggleThicknessPanel
            | Event::CloseThicknessPanel
            | Event::DrawThicknessChanged(_) => Thickness,

            // 自定义精度对话框
            Event::OpenCustomPrecisionDialog
            | Event::CloseCustomPrecisionDialog
            | Event::ConfirmCustomPrecision
            | Event::CustomPrecisionTupletCountChanged(_)
            | Event::CustomPrecisionTupletTypeChanged(_)
            | Event::CustomPrecisionDotTypeChanged(_)
            | Event::CustomPrecisionNoteValueChanged(_)
            | Event::CustomPrecisionDivisorChanged(_) => CustomPrecision,

            // 仪表盘编辑（BPM / PPQ）与自动滚动
            Event::BpmEditToggled(_)
            | Event::BpmEditChanged(_)
            | Event::BpmEditConfirmed
            | Event::PpqEditToggled(_)
            | Event::PpqEditChanged(_)
            | Event::PpqEditConfirmed
            | Event::AutoScrollModeChanged => Dashboard,

            // 工具栏高度拖拽
            Event::ResizeDragStarted(_) | Event::ResizeDragged(_) | Event::ResizeDragEnded => {
                Resize
            }
            // 仅写日志 / 无状态可改的事件
            Event::Undo
            | Event::Redo
            | Event::Quantize
            | Event::ImageToMidiClicked
            | Event::OpenCollaborationDialog
            | Event::OpenProjectSettingsDialog
            | Event::OpenMemoryMonitorDialog
            | Event::SpeedChange
            | Event::FlipVertical
            | Event::FlipHorizontal(_)
            | Event::TransposeUp(_)
            | Event::TransposeDown(_)
            | Event::Split
            | Event::Glue
            | Event::Tie
            | Event::OpenHistoryDialog
            | Event::OpenDrawSettingsDialog => LogOnly,
            #[cfg(lumino_shape_drawer_entry)]
            Event::OpenShapeDrawerDialog => LogOnly,
            // 洋葱皮已由 `update` 提前返回、悬停由 Root 写入状态栏——两者都不改工具栏状态。
            Event::ToggleOnionSkin
            | Event::ToggleOnionSkinMenu
            | Event::CloseOnionSkinMenu
            | Event::OnionSkinModeSelected(_)
            | Event::ButtonHovered(_) => Ignored,
        }
    }

    /// 收到「其它动作」时收起互斥浮层；每个浮层只放行自己的开合 / 内部交互事件。
    ///
    /// 存储只有一个 `Option`：这里只需判断「当前打开的那个是否被本事件放行」。
    /// 粗细面板与文字参数面板的连带关系已由单存储消解——它们不可能同时打开。
    fn dismiss_popovers(&mut self, event: &Event) {
        let keep = popover_keep(event);
        if matches!(keep, PopoverKeep::All) {
            return;
        }
        if let Some(active) = self.active_popover {
            let kept = matches!(keep, PopoverKeep::Only(kept) if kept == active);
            if !kept {
                self.active_popover = None;
            }
        }
    }

    fn update_resize(&mut self, event: Event) {
        match event {
            Event::ResizeDragStarted(_) => self.is_resizing = true,
            Event::ResizeDragEnded => self.is_resizing = false,
            Event::ResizeDragged(_) => {}
            _ => {}
        }
    }

    /// 只写日志的事件：工具栏没有状态可改，但保留可观测性。
    fn log_unhandled(&self, event: Event) {
        match event {
            Event::Undo => tracing::debug!("工具栏: 撤销操作"),
            Event::Redo => tracing::debug!("工具栏: 重做操作"),
            Event::Quantize => tracing::debug!("工具栏: 量化操作"),
            Event::ImageToMidiClicked => {
                tracing::info!("工具栏: 图片转MIDI功能开发中，按钮已点击");
            }
            Event::OpenCollaborationDialog => tracing::debug!("工具栏: 请求打开协作对话框"),
            Event::OpenProjectSettingsDialog => tracing::debug!("工具栏: 请求打开工程设置对话框"),
            Event::OpenMemoryMonitorDialog => tracing::debug!("工具栏: 请求打开内存监控对话框"),
            Event::SpeedChange => tracing::debug!("工具栏: 触发音符变速"),
            Event::FlipVertical => tracing::debug!("工具栏: 触发垂直翻转"),
            Event::FlipHorizontal(_) => tracing::debug!("工具栏: 触发水平翻转"),
            Event::TransposeUp(semitones) => tracing::debug!("工具栏: 触发移调 +{}", semitones),
            Event::TransposeDown(semitones) => tracing::debug!("工具栏: 触发移调 -{}", semitones),
            Event::Split => tracing::debug!("工具栏: 触发音符分割"),
            Event::Glue => tracing::debug!("工具栏: 触发音符合并"),
            Event::Tie => tracing::debug!("工具栏: 触发音符连奏"),
            #[cfg(lumino_shape_drawer_entry)]
            Event::OpenShapeDrawerDialog => tracing::debug!("工具栏: 请求打开形状抽屉"),
            Event::OpenHistoryDialog => tracing::debug!("工具栏: 请求打开撤销历史"),
            // 工具栏自身无状态可改：本事件只是"请求打开独立对话框"的信道，
            // 实际开窗由 `ToolbarHandler` 转 `window::Event::OpenDrawSettingsDialog` 交 Runner。
            // 因此**不动** current_tool / fill_enabled，也不收任何窗口内浮层——
            // 点设置不该顺手把当前绘制工具或已开的分音符填充面板弄没。
            Event::OpenDrawSettingsDialog => {
                tracing::debug!("工具栏: 请求打开音符画设置对话框")
            }
            _ => {}
        }
    }
}

/// 本事件允许保持打开的浮层。
fn popover_keep(event: &Event) -> PopoverKeep {
    use PopoverKeep::{All, None, Only};
    match event {
        // 悬停由鼠标进出触发，不应关闭菜单：否则菜单打开导致重绘、按钮 mouse_area
        // 重新挂载会发出 on_enter，立刻把刚打开的菜单关掉。
        Event::ButtonHovered(_) => All,
        Event::ToggleOverflowMenu | Event::CloseOverflowMenu => Only(Popover::OverflowMenu),
        Event::ToggleBrushDropdown | Event::CloseBrushDropdown => Only(Popover::BrushDropdown),
        Event::ToggleShapeDropdown | Event::CloseShapeDropdown => Only(Popover::ShapeDropdown),
        Event::ToggleOnionSkinMenu
        | Event::CloseOnionSkinMenu
        | Event::OnionSkinModeSelected(_) => Only(Popover::OnionSkinMenu),
        Event::ToggleThicknessPanel
        | Event::CloseThicknessPanel
        | Event::DrawThicknessChanged(_) => Only(Popover::ThicknessPanel),
        Event::CloseTextParamsPanel
        | Event::ToolPanelItemSettingsRequested(ToolPanelItem::Text) => Only(Popover::TextParams),
        _ => None,
    }
}
