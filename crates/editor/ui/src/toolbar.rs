//! Toolbar 模块 - 顶部工具栏组件
//!
//! 该模块已拆分为以下子模块：
//! - `types`: 类型定义（NotePrecision, TupletType, DotType, Tool 等）
//! - `event`: 工具栏事件枚举和工厂方法
//! - `view`: 视图渲染逻辑

pub(crate) mod brush_dropdown;
mod buttons;
pub mod event;
pub(crate) mod overflow;
mod record;
pub(crate) mod tool_panel;
pub mod types;
mod view;

pub use event::{Event, FlipHorizontalMode, ToolPanelItem};
pub use lumino_ui_core::button_descs::ButtonId;
pub use types::{
    CustomPrecisionDialog, DEFAULT_HEIGHT, DotType, MAX_HEIGHT, MIN_HEIGHT, NotePrecision,
    RESIZE_HANDLE_HEIGHT, Tool, TupletType,
};

use crate::util::is_digits_or_empty;
use lumino_core::BrushConfig;

/// 工具栏视图所需的性能/检测数据聚合
///
/// 移植自 yinhe `chrome/transport_bar.rs` 的 `TransportContext`：用结构体替代长参数列表，
/// 避免工具栏渲染函数参数爆炸。
///
/// 注意：CPU/内存性能数据已移至底部状态栏（statusbar）显示，因此此处不再包含
/// `perf_data`；仅保留时间码换算所需的播放位置/PPQ/速度点。
pub struct ToolbarPerfContext<'a> {
    /// 当前播放位置（tick）
    pub playback_tick: f32,
    /// 每四分音符脉冲数（PPQ）
    pub ppq: u16,
    /// 速度变化点（用于 tick→秒 / BPM 换算）
    pub tempo_points: &'a [lumino_note_core::midi_types::TempoPoint],
}

/// 工具栏组件
pub struct Toolbar {
    /// 当前工具
    pub current_tool: Tool,
    /// 是否正在播放
    pub is_playing: bool,
    /// 是否启用循环播放
    pub is_looping: bool,
    /// 是否正在录制
    pub is_recording: bool,
    /// 工具栏高度（默认 72）
    pub height: f32,
    /// 是否正在拖拽调整高度
    is_resizing: bool,
    /// 拖拽开始时的鼠标 Y 坐标
    resize_start_y: f32,
    /// 拖拽开始时的工具栏高度
    resize_start_height: f32,
    /// 当前音符精度设置
    pub note_precision: NotePrecision,
    /// 音符变速速度因子（浮点值，如 0.5 表示半速）
    pub speed_factor: f32,
    /// Ctrl 键是否按下（用于变速按钮的快捷操作）
    pub ctrl_pressed: bool,
    /// Shift 键是否按下（用于翻转按钮的快捷操作）
    pub shift_pressed: bool,
    /// 自定义精度对话框状态
    pub custom_precision_dialog: CustomPrecisionDialog,
    /// 自动滚动模式
    pub auto_scroll_mode: lumino_core::storage::config::AutoScrollMode,
    /// PPQ 编辑模式（true = 正在编辑）
    pub ppq_editing: bool,
    /// PPQ 编辑缓冲区（仅包含数字字符）
    pub ppq_edit_buffer: String,
    /// 溢出菜单是否打开
    pub overflow_menu_open: bool,
    /// 绘制工具选择面板是否打开（颜料桶右侧小三角触发）
    pub tool_panel_open: bool,
    /// 画刷工具下拉是否打开（ctrl+点击附属按钮触发）
    pub brush_dropdown_open: bool,
    /// 画刷工具配置（粗细度 + 每层音轨分配）
    pub brush: BrushConfig,
    /// 颜料桶填充模式开关（仅曲线工具激活时可操作）
    pub fill_enabled: bool,
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
                Event::ToggleBrushDropdown | Event::ButtonHovered(_) | Event::CloseBrushDropdown
            )
        {
            self.brush_dropdown_open = false;
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
                    "工具栏: 绘制工具面板 {}",
                    if self.tool_panel_open {
                        "打开"
                    } else {
                        "关闭"
                    }
                );
            }
            Event::CloseToolPanel => {
                self.tool_panel_open = false;
                tracing::debug!("工具栏: 关闭绘制工具面板");
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
                    ToolPanelItem::FillBucket => {
                        // 填充桶为曲线/形状的共存修饰：
                        // - 当前为曲线/形状时切换填充开关；
                        // - 否则（画刷/文字/橡皮擦等独立工具）切换为曲线 + 填充开启。
                        if matches!(self.current_tool, Tool::Curve | Tool::Shape) {
                            self.fill_enabled = !self.fill_enabled;
                        } else {
                            self.current_tool = Tool::Curve;
                            self.fill_enabled = true;
                        }
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
                        // 橡皮擦：独立工具，不可与填充桶共存
                        self.current_tool = Tool::Eraser;
                        self.fill_enabled = false;
                    }
                }
                // 选中后关闭面板（与溢出菜单逐项选择行为一致）
                self.tool_panel_open = false;
                tracing::debug!("工具栏: 工具面板选择 {:?}", item);
            }
        }
    }

    /// 检查是否正在调整大小
    pub fn is_resizing(&self) -> bool {
        self.is_resizing
    }

    /// 开始调整大小，记录起始鼠标 Y 坐标
    pub fn start_resize(&mut self, cursor_y: f32) {
        self.is_resizing = true;
        self.resize_start_y = cursor_y;
        self.resize_start_height = self.height;
    }

    /// 更新拖拽位置（从外部传入当前鼠标 Y 坐标）
    pub fn update_resize_position(&mut self, cursor_y: f32) {
        if self.is_resizing {
            let delta_y = cursor_y - self.resize_start_y;
            let new_height = self.resize_start_height + delta_y;
            self.height = new_height.clamp(MIN_HEIGHT, MAX_HEIGHT);
        }
    }

    /// 结束调整大小
    pub fn end_resize(&mut self) {
        self.is_resizing = false;
    }

    /// 获取当前高度
    pub fn height(&self) -> f32 {
        self.height
    }
}

impl Default for Toolbar {
    fn default() -> Self {
        Self::new()
    }
}
