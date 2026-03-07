use iced_core::{Alignment, Length, Point};
use iced_widget::{button, container, mouse_area, pick_list, row, space, text};

use crate::{Element, Message, Theme, resources::icon, window};

/// 音符精度/网格对齐设置
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotePrecision {
    /// 全音符 (4拍)
    Whole,
    /// 二分音符 (2拍)
    Half,
    /// 四分音符 (1拍)
    #[default]
    Quarter,
    /// 八分音符 (1/2拍)
    Eighth,
    /// 十六分音符 (1/4拍)
    Sixteenth,
    /// 三十二分音符 (1/8拍)
    ThirtySecond,
    /// 六十四分音符 (1/16拍)
    SixtyFourth,
    /// 128分音符 (1/32拍)
    OneTwentyEighth,
    /// 自定义
    Custom,
}

impl std::fmt::Display for NotePrecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            NotePrecision::Whole => "全音符",
            NotePrecision::Half => "二分音符",
            NotePrecision::Quarter => "四分音符",
            NotePrecision::Eighth => "八分音符",
            NotePrecision::Sixteenth => "十六分音符",
            NotePrecision::ThirtySecond => "三十二分音符",
            NotePrecision::SixtyFourth => "六十四分音符",
            NotePrecision::OneTwentyEighth => "128分音符",
            NotePrecision::Custom => "自定义",
        };
        write!(f, "{}", name)
    }
}

impl NotePrecision {
    /// 获取精度显示名称
    pub fn display_name(&self) -> &'static str {
        match self {
            NotePrecision::Whole => "全音符",
            NotePrecision::Half => "二分音符",
            NotePrecision::Quarter => "四分音符",
            NotePrecision::Eighth => "八分音符",
            NotePrecision::Sixteenth => "十六分音符",
            NotePrecision::ThirtySecond => "三十二分音符",
            NotePrecision::SixtyFourth => "六十四分音符",
            NotePrecision::OneTwentyEighth => "128分音符",
            NotePrecision::Custom => "自定义",
        }
    }

    /// 根据PPQ计算对应的tick值
    pub fn as_ticks(self, ppq: u16) -> f32 {
        let ppq = ppq as f32;
        match self {
            NotePrecision::Whole => ppq * 4.0,
            NotePrecision::Half => ppq * 2.0,
            NotePrecision::Quarter => ppq,
            NotePrecision::Eighth => ppq / 2.0,
            NotePrecision::Sixteenth => ppq / 4.0,
            NotePrecision::ThirtySecond => ppq / 8.0,
            NotePrecision::SixtyFourth => ppq / 16.0,
            NotePrecision::OneTwentyEighth => ppq / 32.0,
            NotePrecision::Custom => ppq / 4.0, // 默认自定义为十六分音符
        }
    }

    /// 获取所有预设选项（不包括自定义）
    pub fn presets() -> &'static [NotePrecision] {
        &[
            NotePrecision::Whole,
            NotePrecision::Half,
            NotePrecision::Quarter,
            NotePrecision::Eighth,
            NotePrecision::Sixteenth,
            NotePrecision::ThirtySecond,
            NotePrecision::SixtyFourth,
            NotePrecision::OneTwentyEighth,
        ]
    }
}

/// 自定义精度对话框状态
#[derive(Debug, Clone)]
pub struct CustomPrecisionDialog {
    pub is_open: bool,
    pub numerator: String,   // 分子（如 1）
    pub denominator: String, // 分母（如 4）
}

impl Default for CustomPrecisionDialog {
    fn default() -> Self {
        Self {
            is_open: false,
            numerator: "1".to_string(),
            denominator: "4".to_string(),
        }
    }
}

impl CustomPrecisionDialog {
    /// 计算对应的tick值（基于PPQ）
    pub fn calculate_ticks(&self, ppq: u16) -> Option<f32> {
        let num = self.numerator.parse::<f32>().ok()?;
        let den = self.denominator.parse::<f32>().ok()?;
        if den == 0.0 {
            return None;
        }
        Some((ppq as f32) * 4.0 * num / den)
    }

    /// 获取显示文本（如 "1/4"）
    pub fn display_text(&self) -> String {
        format!("{}/{}", self.numerator, self.denominator)
    }
}

/// 工具栏事件
#[derive(Debug, Clone)]
pub enum Event {
    Play,
    Pause,
    Stop,
    SkipBackward,
    SkipForward,
    ToolSelected(Tool),
    /// 精度设置变更
    PrecisionChanged(NotePrecision),
    /// 打开自定义精度对话框
    OpenCustomPrecisionDialog,
    /// 关闭自定义精度对话框
    CloseCustomPrecisionDialog,
    /// 确认自定义精度
    ConfirmCustomPrecision,
    /// 自定义精度分子变更
    CustomPrecisionNumeratorChanged(String),
    /// 自定义精度分母变更
    CustomPrecisionDenominatorChanged(String),
    /// 开始拖拽调整高度
    ResizeDragStarted(Point),
    /// 拖拽中调整高度
    ResizeDragged(Point),
    /// 结束拖拽调整高度
    ResizeDragEnded,
}

impl Event {
    pub const fn play() -> Message {
        Message::Toolbar(Self::Play)
    }

    pub const fn pause() -> Message {
        Message::Toolbar(Self::Pause)
    }

    pub const fn stop() -> Message {
        Message::Toolbar(Self::Stop)
    }

    pub const fn skip_backward() -> Message {
        Message::Toolbar(Self::SkipBackward)
    }

    pub const fn skip_forward() -> Message {
        Message::Toolbar(Self::SkipForward)
    }

    pub const fn tool_selected(tool: Tool) -> Message {
        Message::Toolbar(Self::ToolSelected(tool))
    }

    pub const fn precision_changed(precision: NotePrecision) -> Message {
        Message::Toolbar(Self::PrecisionChanged(precision))
    }

    pub const fn open_custom_precision_dialog() -> Message {
        Message::Toolbar(Self::OpenCustomPrecisionDialog)
    }

    pub const fn close_custom_precision_dialog() -> Message {
        Message::Toolbar(Self::CloseCustomPrecisionDialog)
    }

    pub const fn confirm_custom_precision() -> Message {
        Message::Toolbar(Self::ConfirmCustomPrecision)
    }

    pub fn custom_precision_numerator_changed(value: String) -> Message {
        Message::Toolbar(Self::CustomPrecisionNumeratorChanged(value))
    }

    pub fn custom_precision_denominator_changed(value: String) -> Message {
        Message::Toolbar(Self::CustomPrecisionDenominatorChanged(value))
    }

    pub fn resize_drag_started() -> Message {
        Message::Toolbar(Self::ResizeDragStarted(Point::new(0.0, 0.0)))
    }

    pub fn resize_dragged() -> Message {
        Message::Toolbar(Self::ResizeDragged(Point::new(0.0, 0.0)))
    }

    pub const fn resize_drag_ended() -> Message {
        Message::Toolbar(Self::ResizeDragEnded)
    }
}

/// 工具类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    #[default]
    Pointer,
    Pencil,
    Brush,
    Pen,
    Eraser,
    Razor,
}

pub struct Toolbar {
    pub current_tool: Tool,
    pub is_playing: bool,
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
}

/// 工具栏默认高度
const DEFAULT_HEIGHT: f32 = 72.0;
/// 最小高度
const MIN_HEIGHT: f32 = 56.0;
/// 最大高度
const MAX_HEIGHT: f32 = 200.0;
/// 拖拽手柄高度
const RESIZE_HANDLE_HEIGHT: f32 = 6.0;

impl Toolbar {
    pub fn new() -> Self {
        Self {
            current_tool: Tool::default(),
            is_playing: false,
            height: DEFAULT_HEIGHT,
            is_resizing: false,
            resize_start_y: 0.0,
            resize_start_height: DEFAULT_HEIGHT,
            note_precision: NotePrecision::default(),
            custom_precision_dialog: CustomPrecisionDialog::default(),
        }
    }

    pub fn view<'a>(&'a self, window: &'a window::Window) -> Element<'a> {
        let palette = window.theme.extended_palette();

        // 计算内容区域高度（总高度减去手柄高度）
        let content_height = self.height - RESIZE_HANDLE_HEIGHT;

        // 播放控制区域 (132px 宽)
        let playback_controls = container(
            row![
                tool_button(icon::SkipBackward, Event::skip_backward(), window),
                space().width(4),
                if self.is_playing {
                    tool_button(icon::Pause, Event::pause(), window)
                } else {
                    tool_button(icon::Play, Event::play(), window)
                },
                space().width(4),
                tool_button(icon::SkipForward, Event::skip_forward(), window),
            ]
            .align_y(Alignment::Center),
        )
        .width(132)
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .align_x(iced_core::alignment::Horizontal::Center)
        .style(move |_theme: &Theme| {
            container::Style::default()
                .background(palette.background.weak.color)
                .border(iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                })
        });

        // 工具选择区域 (285px 宽)
        let tools = container(
            row![
                tool_selector(icon::MousePointer, Tool::Pointer, self.current_tool, window),
                space().width(4),
                tool_selector(icon::Pencil, Tool::Pencil, self.current_tool, window),
                space().width(4),
                tool_selector(icon::Eraser, Tool::Eraser, self.current_tool, window),
            ]
            .align_y(Alignment::Center),
        )
        .width(285)
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .align_x(iced_core::alignment::Horizontal::Center)
        .style(move |_theme: &Theme| {
            container::Style::default()
                .background(palette.background.weak.color)
                .border(iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                })
        });

        // 调整大小手柄区域
        let resize_handle: Element<'_> = mouse_area(
            container(space().height(Length::Fixed(RESIZE_HANDLE_HEIGHT)))
                .width(Length::Fill)
                .style(move |_theme: &Theme| {
                    container::Style::default()
                        .background(if self.is_resizing {
                            palette.primary.strong.color
                        } else {
                            palette.background.weakest.color
                        })
                }),
        )
        .interaction(iced_core::mouse::Interaction::ResizingVertically)
        .on_press(Message::Toolbar(Event::ResizeDragStarted(Point::new(0.0, 0.0))))
        .on_release(Message::Toolbar(Event::ResizeDragEnded))
        .into();

        // 精度设置区域
        let precision_options: Vec<NotePrecision> = NotePrecision::presets()
            .iter()
            .copied()
            .chain(std::iter::once(NotePrecision::Custom))
            .collect();

        let precision_selector = container(
            row![
                text("精度:").size(14),
                space().width(8),
                pick_list(
                    precision_options,
                    Some(self.note_precision),
                    |precision| {
                        if precision == NotePrecision::Custom {
                            // 选择自定义时，发送消息到Root打开对话框
                            Message::OpenCustomPrecisionDialog
                        } else {
                            Event::precision_changed(precision)
                        }
                    },
                )
                .placeholder("选择精度")
                .padding([4, 8])
                .width(Length::Fixed(120.0)),
            ]
            .align_y(Alignment::Center),
        )
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .padding([0, 16])
        .style(move |_theme: &Theme| {
            container::Style::default().background(palette.background.weakest.color)
        });

        // 主工具栏内容 - 横向排列所有区域
        let toolbar_content = container(
            row![playback_controls, space().width(16), tools, space().width(16), precision_selector]
                .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fixed(content_height))
        .padding([8, 16])
        .style(move |_theme: &Theme| {
            container::Style::default().background(palette.background.weakest.color)
        });

        // 组合工具栏内容和调整手柄
        iced_widget::column![toolbar_content, resize_handle]
            .width(Length::Fill)
            .height(Length::Fixed(self.height))
            .into()
    }

    pub fn update(&mut self, event: Event) {
        match event {
            Event::Play => self.is_playing = true,
            Event::Pause => self.is_playing = false,
            Event::Stop => self.is_playing = false,
            Event::SkipBackward => {}
            Event::SkipForward => {}
            Event::ToolSelected(tool) => self.current_tool = tool,
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
                // 确认自定义精度，此时Toolbar会保持Custom状态
                // 实际的tick计算在Root中处理
                self.custom_precision_dialog.is_open = false;
                tracing::debug!("工具栏: 确认自定义精度");
            }
            Event::CustomPrecisionNumeratorChanged(value) => {
                // 只接受数字输入
                if value.chars().all(|c| c.is_ascii_digit()) || value.is_empty() {
                    self.custom_precision_dialog.numerator = value;
                }
            }
            Event::CustomPrecisionDenominatorChanged(value) => {
                // 只接受数字输入
                if value.chars().all(|c| c.is_ascii_digit()) || value.is_empty() {
                    self.custom_precision_dialog.denominator = value;
                }
            }
            Event::ResizeDragStarted(_) => {
                // 拖拽开始由 Host 处理，这里只需要标记状态
                self.is_resizing = true;
            }
            Event::ResizeDragged(_) => {
                // 拖拽中的位置更新由 Host 通过 update_resize_position 处理
            }
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

/// 工具按钮
fn tool_button<'a>(
    icon_enum: icon::Icon,
    on_press: Message,
    window: &'a window::Window,
) -> Element<'a> {
    button(icon::view_with_size_and_theme(
        icon_enum,
        20,
        20,
        Some(&window.theme),
    ))
    .on_press(on_press)
    .style(|_theme: &Theme, _status| {
        button::Style::default().with_background(iced_core::Color::TRANSPARENT)
    })
    .padding(4)
    .into()
}

/// 工具选择器
fn tool_selector<'a>(
    icon_enum: icon::Icon,
    tool: Tool,
    current_tool: Tool,
    window: &'a window::Window,
) -> Element<'a> {
    let is_selected = tool == current_tool;
    let palette = window.theme.extended_palette();

    button(icon::view_with_size_and_theme(
        icon_enum,
        17,
        17,
        Some(&window.theme),
    ))
    .on_press(Event::tool_selected(tool))
    .style(move |_theme: &Theme, status| {
        let bg = if is_selected {
            palette.background.strong.color
        } else if status == iced_widget::button::Status::Hovered {
            palette.background.weak.color
        } else {
            iced_core::Color::TRANSPARENT
        };

        button::Style {
            border: iced_core::Border {
                radius: 3.0.into(),
                width: 0.0,
                color: iced_core::Color::TRANSPARENT,
            },
            ..Default::default()
        }
        .with_background(bg)
    })
    .padding(iced_core::Padding::new(4.0))
    .into()
}
