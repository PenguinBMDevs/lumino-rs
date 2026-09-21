use super::*;
use crate::util::is_digits_or_empty;

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
            note_precision: NotePrecision::default(),
            speed_factor: 0.5,
            ctrl_pressed: false,
            shift_pressed: false,
            custom_precision_dialog: CustomPrecisionDialog::default(),
            auto_scroll_mode: lumino_core::storage::config::AutoScrollMode::default(),
            ppq_editing: false,
            ppq_edit_buffer: String::new(),
            overflow_menu_open: false,
            tool_panel_open: false,
            brush_dropdown_open: false,
            brush: BrushConfig::new(),
            fill_enabled: false,
            shape_dropdown_open: false,
            current_shape: ShapeType::default(),
        }
    }

    /// 更新工具栏状态
    pub fn update(&mut self, event: Event) {
        // 菜单打开时，除以下情况外其余操作先关闭菜单：
        // - 再次点击“更多”按钮（ToggleOverflowMenu）用于切换关闭
        // - 悬停事件（ButtonHovered）由鼠标进出触发，不应关闭菜单，
        //   否则菜单打开导致重绘、按钮 mouse_area 重新挂载会发出 on_enter，
        //   立刻把刚打开的菜单关掉（表现为“更多面板打不开”）
        // - 显式关闭事件（CloseOverflowMenu）
        if self.overflow_menu_open
            && !matches!(
                event,
                Event::ToggleOverflowMenu | Event::ButtonHovered(_) | Event::CloseOverflowMenu
            )
        {
            self.overflow_menu_open = false;
        }

        // 绘制工具选择面板打开时，除以下情况外其余操作先关闭面板：
        // - 再次点击小三角（ToggleToolPanel）用于切换关闭
        // - 悬停事件（ButtonHovered）不应关闭面板（同溢出菜单的处理）
        // - 显式关闭事件（CloseToolPanel）
        if self.tool_panel_open
            && !matches!(
                event,
                Event::ToggleToolPanel | Event::ButtonHovered(_) | Event::CloseToolPanel
            )
        {
            self.tool_panel_open = false;
        }

        // 画刷工具下拉打开时，除以下情况外其余操作先关闭下拉：
        // - 再次点击附属按钮（ToggleBrushDropdown）用于切换关闭
        // - 悬停事件不应关闭下拉
        // - 显式关闭事件（CloseBrushDropdown）
        if self.brush_dropdown_open
            && !matches!(
                event,
                Event::ToggleBrushDropdown
                    | Event::ButtonHovered(_)
                    | Event::CloseBrushDropdown
                    | Event::BrushThicknessChanged(_)
            )
        {
            self.brush_dropdown_open = false;
        }

        // 形状工具下拉打开时，除以下情况外其余操作先关闭下拉：
        // - 再次点击形状工具（ToggleShapeDropdown）用于切换关闭
        // - 悬停事件不应关闭下拉
        // - 显式关闭事件（CloseShapeDropdown）
        // - 选中某个图形（ShapeTypeSelected）即视为完成一次选择，下拉随之关闭
        if self.shape_dropdown_open
            && !matches!(
                event,
                Event::ToggleShapeDropdown | Event::ButtonHovered(_) | Event::CloseShapeDropdown
            )
        {
            self.shape_dropdown_open = false;
        }

        match event {
            Event::Play => self.is_playing = true,
            Event::Pause => self.is_playing = false,
            Event::Stop => self.is_playing = false,
            Event::SkipBackward => {}
            Event::SkipForward => {}
            Event::Undo => {
                tracing::debug!("工具栏: 撤销操作");
            }
            Event::Redo => {
                tracing::debug!("工具栏: 重做操作");
            }
            Event::ToolSelected(tool) => {
                self.current_tool = tool;
                // 切换工具即离开任何共存态：填充桶仅曲线/形状可共存，切到其它工具一律关闭
                self.fill_enabled = false;
                // 关闭所有下拉，避免工具切换后残留
                self.tool_panel_open = false;
                self.brush_dropdown_open = false;
                self.shape_dropdown_open = false;
            }
            Event::FillToggled(enabled) => {
                self.fill_enabled = enabled;
                tracing::debug!("工具栏: 颜料桶填充模式切换为 {}", enabled);
            }
            Event::Quantize => {
                tracing::debug!("工具栏: 量化操作");
            }
            Event::PrecisionChanged(precision) => {
                self.note_precision = precision;
                tracing::debug!("工具栏: 精度设置变更为 {:?}", precision);
            }
            Event::OpenCustomPrecisionDialog => {
                self.custom_precision_dialog.is_open = true;
                tracing::debug!("工具栏: 打开自定义精度对话框");
            }
            Event::CloseCustomPrecisionDialog => {
                self.custom_precision_dialog.is_open = false;
                tracing::debug!("工具栏: 关闭自定义精度对话框");
            }
            Event::ConfirmCustomPrecision => {
                self.custom_precision_dialog.is_open = false;
                tracing::debug!("工具栏: 确认自定义精度");
            }
            Event::CustomPrecisionTupletCountChanged(value) => {
                if is_digits_or_empty(&value) {
                    self.custom_precision_dialog.tuplet_count = value;
                }
            }
            Event::CustomPrecisionTupletTypeChanged(value) => {
                self.custom_precision_dialog.tuplet_type = value;
                self.custom_precision_dialog.tuplet_count = value.value().to_string();
            }
            Event::CustomPrecisionDotTypeChanged(value) => {
                self.custom_precision_dialog.dot_type = value;
            }
            Event::CustomPrecisionNoteValueChanged(value) => {
                if is_digits_or_empty(&value) {
                    self.custom_precision_dialog.note_value = value;
                }
            }
            Event::CustomPrecisionDivisorChanged(value) => {
                if is_digits_or_empty(&value) {
                    self.custom_precision_dialog.divisor = value;
                }
            }
            Event::OpenCollaborationDialog => {
                tracing::debug!("工具栏: 请求打开协作对话框");
            }
            Event::OpenProjectSettingsDialog => {
                tracing::debug!("工具栏: 请求打开工程设置对话框");
            }
            Event::OpenMemoryMonitorDialog => {
                tracing::debug!("工具栏: 请求打开内存监控对话框");
            }
            Event::AutoScrollModeChanged => {
                self.auto_scroll_mode = match self.auto_scroll_mode {
                    lumino_core::storage::config::AutoScrollMode::FixedIndicatorLeft => {
                        lumino_core::storage::config::AutoScrollMode::ScrollingIndicator
                    }
                    lumino_core::storage::config::AutoScrollMode::ScrollingIndicator => {
                        lumino_core::storage::config::AutoScrollMode::Off
                    }
                    lumino_core::storage::config::AutoScrollMode::Off => {
                        lumino_core::storage::config::AutoScrollMode::FixedIndicatorLeft
                    }
                };
                tracing::debug!("工具栏: 自动滚动模式切换为 {:?}", self.auto_scroll_mode);
            }
            Event::ToggleLoop => {
                self.is_looping = !self.is_looping;
                tracing::debug!("工具栏: 循环播放切换为 {}", self.is_looping);
            }
            Event::Record => {
                self.is_recording = true;
                tracing::debug!("工具栏: 开始录制");
            }
            Event::RecordStop => {
                self.is_recording = false;
                tracing::debug!("工具栏: 停止录制");
            }
            Event::SpeedChange => {
                tracing::debug!("工具栏: 触发音符变速");
            }
            Event::FlipVertical => {
                tracing::debug!("工具栏: 触发垂直翻转");
            }
            Event::FlipHorizontal(_) => {
                tracing::debug!("工具栏: 触发水平翻转");
            }
            Event::TransposeUp(semitones) => {
                tracing::debug!("工具栏: 触发移调 +{}", semitones);
            }
            Event::TransposeDown(semitones) => {
                tracing::debug!("工具栏: 触发移调 -{}", semitones);
            }
            Event::Split => {
                tracing::debug!("工具栏: 触发音符分割");
            }
            Event::Glue => {
                tracing::debug!("工具栏: 触发音符合并");
            }
            Event::Tie => {
                tracing::debug!("工具栏: 触发音符连奏");
            }
            Event::ToggleOverflowMenu => {
                self.overflow_menu_open = !self.overflow_menu_open;
                // 与绘制工具面板互斥：打开溢出菜单时关闭工具面板
                self.tool_panel_open = false;
                tracing::debug!(
                    "工具栏: 溢出菜单 {}",
                    if self.overflow_menu_open {
                        "打开"
                    } else {
                        "关闭"
                    }
                );
            }
            Event::CloseOverflowMenu => {
                self.overflow_menu_open = false;
                tracing::debug!("工具栏: 关闭溢出菜单");
            }
            Event::ResizeDragStarted(_) => {
                self.is_resizing = true;
            }
            Event::ResizeDragged(_) => {}
            Event::ResizeDragEnded => {
                self.is_resizing = false;
            }
            Event::PpqEditToggled(current_ppq) => {
                if self.ppq_editing {
                    // 已在编辑状态 → 取消编辑
                    self.ppq_editing = false;
                    self.ppq_edit_buffer.clear();
                } else {
                    // 进入编辑状态，用当前 PPQ 值初始化缓冲区
                    self.ppq_editing = true;
                    self.ppq_edit_buffer = current_ppq.to_string();
                }
            }
            Event::PpqEditChanged(value) => {
                if self.ppq_editing {
                    // 只允许输入数字
                    if value.is_empty() || value.chars().all(|c| c.is_ascii_digit()) {
                        self.ppq_edit_buffer = value;
                    }
                }
            }
            Event::PpqEditConfirmed => {
                self.ppq_editing = false;
                self.ppq_edit_buffer.clear();
            }
            // 悬停描述事件：工具栏自身不处理，交由 Root 写入底部状态栏
            Event::ButtonHovered(_) => {}
            Event::ImageToMidiClicked => {
                tracing::info!("工具栏: 图片转MIDI功能开发中，按钮已点击");
            }
            Event::ToggleToolPanel => {
                self.tool_panel_open = !self.tool_panel_open;
                // 与溢出菜单、画刷下拉互斥：打开工具面板时关闭其余浮层
                self.overflow_menu_open = false;
                self.brush_dropdown_open = false;
                tracing::debug!(
                    "工具栏: 音符绘制工具集 {}",
                    if self.tool_panel_open {
                        "打开"
                    } else {
                        "关闭"
                    }
                );
            }
            Event::CloseToolPanel => {
                self.tool_panel_open = false;
                tracing::debug!("工具栏: 关闭音符绘制工具集");
            }
            Event::ToggleBrushDropdown => {
                self.brush_dropdown_open = !self.brush_dropdown_open;
                // 与其他面板互斥
                self.overflow_menu_open = false;
                self.tool_panel_open = false;
                tracing::debug!(
                    "工具栏: 画刷工具下拉 {}",
                    if self.brush_dropdown_open {
                        "打开"
                    } else {
                        "关闭"
                    }
                );
            }
            Event::CloseBrushDropdown => {
                self.brush_dropdown_open = false;
                tracing::debug!("工具栏: 关闭画刷工具下拉");
            }
            Event::ToggleShapeDropdown => {
                self.shape_dropdown_open = !self.shape_dropdown_open;
                // 与其他面板互斥：打开形状工具下拉时关闭其余浮层
                self.overflow_menu_open = false;
                self.tool_panel_open = false;
                self.brush_dropdown_open = false;
                tracing::debug!(
                    "工具栏: 形状工具下拉 {}",
                    if self.shape_dropdown_open {
                        "打开"
                    } else {
                        "关闭"
                    }
                );
            }
            Event::CloseShapeDropdown => {
                self.shape_dropdown_open = false;
                tracing::debug!("工具栏: 关闭形状工具下拉");
            }
            Event::ShapeTypeSelected(shape) => {
                // 切换当前图形类型（矩形/圆形/三角形）并持久保存到状态变量；
                // 下拉由上方 guard 在收到本事件时自动关闭（视为一次选择完成）。
                self.current_shape = shape;
                tracing::debug!("工具栏: 形状类型切换为 {:?}", shape);
            }
            Event::BrushThicknessChanged(thickness) => {
                self.brush.set_thickness(thickness);
                tracing::debug!("工具栏: 画刷粗细度变更为 {}", self.brush.thickness);
            }
            Event::ToolPanelItemSelected(item) => {
                match item {
                    ToolPanelItem::StrokeSettings => {
                        // 描边设置：功能开发中（UI 占位）
                        tracing::info!("工具栏: 描边设置（功能开发中）");
                    }
                    ToolPanelItem::Curve => {
                        // 曲线工具：独立基础工具，选中后关闭填充共存态
                        // （填充由「填充桶」条目单独开启）
                        self.current_tool = Tool::Curve;
                        self.fill_enabled = false;
                    }
                    ToolPanelItem::FillBucket => {
                        // 颜料桶随时可切换：仅对曲线/形状绘制的封闭图形生效，
                        // 即使当前不在曲线工具也可开启，作用范围由编辑器侧控制。
                        self.fill_enabled = !self.fill_enabled;
                    }
                    ToolPanelItem::Brush => {
                        // 画刷仅可独立使用，不可与填充桶共存
                        self.current_tool = Tool::Brush;
                        self.fill_enabled = false;
                    }
                    ToolPanelItem::Shape => {
                        // 形状工具：与曲线互斥（单一 base 工具），可与填充桶共存，
                        // 选中形状时先关闭填充，再由「填充桶」条目按需开启
                        self.current_tool = Tool::Shape;
                        self.fill_enabled = false;
                    }
                    ToolPanelItem::Text => {
                        // 文字工具：独立工具，不可与任何工具/填充桶共存
                        self.current_tool = Tool::Text;
                        self.fill_enabled = false;
                    }
                    ToolPanelItem::Eraser => {
                        // 绘制橡皮擦：独立于普通编辑橡皮擦（Tool::Eraser），
                        // 专用于曲线/形状/画刷绘制上下文
                        self.current_tool = Tool::DrawEraser;
                        self.fill_enabled = false;
                    }
                }
                // 选中后关闭面板（与溢出菜单逐项选择行为一致）
                self.tool_panel_open = false;
                tracing::debug!("工具栏: 工具面板选择 {:?}", item);
            }
        }
    }
}
