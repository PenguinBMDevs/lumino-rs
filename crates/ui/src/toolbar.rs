//! Toolbar 模块 - 顶部工具栏组件
//!
//! 该模块已拆分为以下子模块：
//! - `types`: 类型定义（NotePrecision, TupletType, DotType, Tool 等）
//! - `event`: 工具栏事件枚举和工厂方法
//! - `view`: 视图渲染逻辑

pub mod event;
pub mod types;
mod view;

pub use event::Event;
pub use types::{
    CustomPrecisionDialog, DEFAULT_HEIGHT, DotType, MAX_HEIGHT, MIN_HEIGHT, NotePrecision,
    RESIZE_HANDLE_HEIGHT, Tool, TupletType,
};

/// 工具栏组件
pub struct Toolbar {
    pub current_tool: Tool,
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
    /// 自定义精度对话框状态
    pub custom_precision_dialog: CustomPrecisionDialog,
    /// 自动滚动模式
    pub auto_scroll_mode: lumino_core::storage::config::AutoScrollMode,
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
            custom_precision_dialog: CustomPrecisionDialog::default(),
            auto_scroll_mode: lumino_core::storage::config::AutoScrollMode::default(),
        }
    }

    /// 更新工具栏状态
    pub fn update(&mut self, event: Event) {
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
            Event::ToolSelected(tool) => self.current_tool = tool,
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
                if value.chars().all(|c| c.is_ascii_digit()) || value.is_empty() {
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
                if value.chars().all(|c| c.is_ascii_digit()) || value.is_empty() {
                    self.custom_precision_dialog.note_value = value;
                }
            }
            Event::CustomPrecisionDivisorChanged(value) => {
                if value.chars().all(|c| c.is_ascii_digit()) || value.is_empty() {
                    self.custom_precision_dialog.divisor = value;
                }
            }
            Event::OpenCollaborationDialog => {
                tracing::debug!("工具栏: 请求打开协作对话框");
            }
            Event::OpenProjectSettingsDialog => {
                tracing::debug!("工具栏: 请求打开工程设置对话框");
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
            Event::ResizeDragStarted(_) => {
                self.is_resizing = true;
            }
            Event::ResizeDragged(_) => {}
            Event::ResizeDragEnded => {
                self.is_resizing = false;
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
