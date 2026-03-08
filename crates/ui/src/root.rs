use iced_core::Length;
use iced_widget::{column, container, progress_bar, row, text};
use lumino_gfx::NoteInstance;

use crate::{
    editor, editor::note::Note, message, settings, sidebar, statusbar, titlebar, toolbar, window,
};
use lumino_core::storage::config::UiConfig;

pub type Message = message::Message;
pub type Theme = iced_core::Theme;
pub type Renderer = iced_wgpu::Renderer;
pub type Element<'a> = iced_core::Element<'a, Message, Theme, Renderer>;

pub struct Root {
    sidebar: sidebar::Sidebar,
    titlebar: titlebar::Titlebar,
    statusbar: statusbar::StatusBar,
    pub toolbar: toolbar::Toolbar,
    pub editor: editor::Editor,
    window: window::Window,
    settings: settings::SettingsPanel,
    progress: Option<(String, f64)>,
    is_progress_window: bool,
    /// 是否有菜单/下拉框打开（打开时不渲染预览音符）
    is_menu_open: bool,
    /// 对话框结果（用于独立窗口模式）
    dialog_result: Option<crate::host::DialogResult>,
    /// 是否是对话框窗口（用于自定义精度对话框等）
    is_dialog_window: bool,
    /// 对话框类型
    dialog_type: DialogType,
    /// 协作对话框状态
    collaboration_dialog: CollaborationDialog,
}

/// 对话框类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DialogType {
    #[default]
    None,
    CustomPrecision,
    Collaboration,
}

/// 协作对话框状态
#[derive(Debug, Clone)]
pub struct CollaborationDialog {
    pub is_open: bool,
    /// 服务器地址
    pub server_host: String,
    /// 服务器端口
    pub server_port: String,
    /// 用户名
    pub username: String,
    /// 房间名称（创建房间用）
    pub room_name: String,
    /// 邀请码（加入房间用）
    pub invite_code: String,
    /// 当前视图状态
    pub view_state: CollaborationViewState,
    /// 连接状态
    pub connection_status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CollaborationViewState {
    #[default]
    Connect,      // 连接服务器界面
    RoomActions,  // 创建/加入房间界面
    InRoom,       // 在房间内界面
}

impl CollaborationDialog {
    pub fn new() -> Self {
        Self {
            is_open: false,
            server_host: "localhost".to_string(),
            server_port: "3000".to_string(),
            username: "用户".to_string(),
            room_name: "我的房间".to_string(),
            invite_code: String::new(),
            view_state: CollaborationViewState::Connect,
            connection_status: String::new(),
        }
    }

    pub fn reset(&mut self) {
        self.is_open = false;
        self.view_state = CollaborationViewState::Connect;
        self.connection_status.clear();
    }
}

impl Root {
    pub fn new(ui_config: &UiConfig) -> Self {
        Self {
            sidebar: sidebar::Sidebar::new(),
            titlebar: titlebar::Titlebar::new(),
            statusbar: statusbar::StatusBar::new(),
            toolbar: toolbar::Toolbar::new(),
            editor: editor::Editor::new(),
            window: window::Window::new(&ui_config.theme),
            settings: settings::SettingsPanel::new(ui_config),
            progress: None,
            is_progress_window: false,
            is_menu_open: false,
            dialog_result: None,
            is_dialog_window: false,
            dialog_type: DialogType::None,
            collaboration_dialog: CollaborationDialog::new(),
        }
    }

    pub fn new_progress(theme: &str) -> Self {
        // 进度窗口使用默认配置
        let default_config = UiConfig::default();
        Self {
            sidebar: sidebar::Sidebar::new(),
            titlebar: titlebar::Titlebar::new(),
            statusbar: statusbar::StatusBar::new(),
            toolbar: toolbar::Toolbar::new(),
            editor: editor::Editor::new(),
            window: window::Window::new(theme),
            settings: settings::SettingsPanel::new(&default_config),
            progress: None,
            is_progress_window: true,
            is_menu_open: false,
            dialog_result: None,
            is_dialog_window: false,
            dialog_type: DialogType::None,
            collaboration_dialog: CollaborationDialog::new(),
        }
    }

    pub fn new_dialog(theme: &str) -> Self {
        // 对话框窗口使用默认配置
        let default_config = UiConfig::default();
        Self {
            sidebar: sidebar::Sidebar::new(),
            titlebar: titlebar::Titlebar::new(),
            statusbar: statusbar::StatusBar::new(),
            toolbar: toolbar::Toolbar::new(),
            editor: editor::Editor::new(),
            window: window::Window::new(theme),
            settings: settings::SettingsPanel::new(&default_config),
            progress: None,
            is_progress_window: false,
            is_menu_open: false,
            dialog_result: None,
            is_dialog_window: true,
            dialog_type: DialogType::CustomPrecision,
            collaboration_dialog: CollaborationDialog::new(),
        }
    }

    pub fn update(&mut self, msg: Message) {
        match msg {
            Message::Core(r) => {
                // 当执行菜单操作时，关闭菜单
                self.set_menu_open(false);
                lumino_core::event::emit(r);
            }
            Message::Window(r) => {
                // 检测主题是否变化，主题变化时需要清除 grid_cache
                let is_theme_change = matches!(r, window::Event::Theme(_));
                self.window.update(r);
                if is_theme_change {
                    self.editor.grid_cache.clear();
                }
            }
            Message::Sidebar(r) => {
                // 先检查是否是音轨切换（避免所有权问题）
                let track_selected_idx = if let sidebar::Event::TrackSelected(idx) = &r {
                    Some(*idx)
                } else {
                    None
                };

                self.sidebar.update(r);

                // 侧边栏显示状态变化，直接设置 canvas offset 为 sidebar 宽度
                let sidebar_width = self.sidebar.width() as f32;
                let current_offset = self.editor.canvas_offset;
                self.editor
                    .set_canvas_offset(iced_core::Point::new(sidebar_width, current_offset.y));

                // 如果是音轨切换，发送 Core 事件通知 Runner 加载对应音轨的音符
                if let Some(track_idx) = track_selected_idx {
                    tracing::debug!("Root: emitting TrackSelected event for track {}", track_idx);
                    lumino_core::event::emit(lumino_core::event::Event::Menu(
                        lumino_core::event::menu::Event::File(
                            lumino_core::event::menu::file::Event::TrackSelected(track_idx),
                        ),
                    ));
                }
            }
            Message::Progress(p) => self.progress = p,
            Message::ScrollbarScrolled(new_scroll_x) => {
                // 处理水平滚动条滚动
                self.editor.set_scroll_x(new_scroll_x);
            }
            Message::ScrollbarScrolledY(new_scroll_y) => {
                // 处理垂直滚动条滚动
                self.editor.set_scroll_y(new_scroll_y);
            }
            Message::ZoomXChanged { zoom, fixed_ratio } => {
                self.editor.set_zoom_x(zoom, fixed_ratio);
            }
            Message::ZoomYChanged { zoom, fixed_ratio } => {
                self.editor.set_zoom_y(zoom, fixed_ratio);
            }
            Message::CanvasBoundsChanged { offset, size } => {
                // 更新 Canvas 偏移量和尺寸
                self.editor.set_canvas_offset(offset);
                self.editor
                    .set_canvas_size(iced_core::Point::new(size.width, size.height));
            }
            Message::EditorAction(action) => {
                self.editor.handle_action(action);
            }
            Message::AudioAction(_action) => {
                // 音频动作处理（留给外层实现）
            }
            // 菜单状态更新
            Message::MenuStateChanged(is_open) => {
                self.set_menu_open(is_open);
            }
            // 设置面板事件
            Message::Settings(event) => {
                self.settings.update(event);
            }
            // ToggleSettings 消息已废弃，设置通过侧边栏路由切换
            Message::ToggleSettings => {}
            // 工具栏事件
            Message::Toolbar(event) => {
                // 如果工具切换了，同步更新 editor 的工具状态
                if let crate::toolbar::Event::ToolSelected(tool) = &event {
                    self.editor.set_tool(*tool);
                }
                // 如果精度设置变更了，同步更新 editor 的 snap_precision
                if let crate::toolbar::Event::PrecisionChanged(precision) = &event {
                    let ticks = (*precision).as_ticks(self.editor.state.ppq);
                    self.editor.state.snap_precision = ticks;
                    self.editor.state.default_note_length = ticks;
                    tracing::debug!("Root: 音符精度同步为 {} ticks (PPQ={})", ticks, self.editor.state.ppq);
                }
                // 处理打开协作对话框事件
                if let crate::toolbar::Event::OpenCollaborationDialog = &event {
                    tracing::info!("Root: 触发打开协作对话框");
                    lumino_core::event::emit(lumino_core::event::Event::Window(
                        lumino_core::event::window::Event::OpenCollaborationDialog,
                    ));
                }
                self.toolbar.update(event);
            }
            // 显式丢弃它
            Message::Null => (),
            // 协作对话框事件
            Message::OpenCollaborationDialog => {
                // 触发外部协作对话框窗口（通过 Core 事件）
                lumino_core::event::emit(lumino_core::event::Event::Window(
                    lumino_core::event::window::Event::OpenCollaborationDialog,
                ));
            }
            Message::CloseCollaborationDialog => {
                if self.is_dialog_window {
                    lumino_core::event::emit(lumino_core::event::Event::Window(
                        lumino_core::event::window::Event::CloseCollaborationDialog,
                    ));
                }
            }
            Message::CollaborationConnect { host, port, username, invite_code } => {
                tracing::info!("协作: 连接服务器 {}:{}", host, port);
                lumino_core::event::emit(lumino_core::event::Event::Window(
                    lumino_core::event::window::Event::CollaborationConnect { host, port, username, invite_code },
                ));
            }
            Message::CollaborationCreateRoom { name } => {
                tracing::info!("协作: 创建房间 {}", name);
                lumino_core::event::emit(lumino_core::event::Event::Window(
                    lumino_core::event::window::Event::CollaborationCreateRoom { name },
                ));
            }
            Message::CollaborationJoinRoom { invite_code } => {
                tracing::info!("协作: 加入房间 {}", invite_code);
                lumino_core::event::emit(lumino_core::event::Event::Window(
                    lumino_core::event::window::Event::CollaborationJoinRoom { invite_code },
                ));
            }
            Message::CollaborationDisconnect => {
                tracing::info!("协作: 断开连接");
                lumino_core::event::emit(lumino_core::event::Event::Window(
                    lumino_core::event::window::Event::CollaborationDisconnect,
                ));
                self.collaboration_dialog.reset();
            }
            Message::CollaborationHostChanged(host) => {
                self.collaboration_dialog.server_host = host;
            }
            Message::CollaborationPortChanged(port) => {
                self.collaboration_dialog.server_port = port;
            }
            Message::CollaborationUsernameChanged(username) => {
                self.collaboration_dialog.username = username;
            }
            Message::CollaborationRoomNameChanged(name) => {
                self.collaboration_dialog.room_name = name;
            }
            Message::CollaborationInviteCodeChanged(code) => {
                self.collaboration_dialog.invite_code = code;
            }
            Message::CollaborationCopyInviteCode => {
                let invite_code = self.collaboration_dialog.invite_code.clone();
                if !invite_code.is_empty() {
                    // 复制到剪贴板
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        if let Err(e) = clipboard.set_text(&invite_code) {
                            tracing::error!("复制邀请码失败: {}", e);
                        } else {
                            tracing::info!("邀请码已复制: {}", invite_code);
                        }
                    }
                }
            }
            Message::CollaborationRemoteMouseMoved { user_id, x, y, color } => {
                self.editor.update_remote_cursor(user_id, iced_core::Point::new(x, y), color);
            }
            Message::CollaborationRemoteNoteUpdate { user_id, operation } => {
                tracing::info!("协作: 处理远端音符更新 - 用户: {}, 操作: {}", user_id, operation);
                // 这里将来可以解析 JSON 并应用到编辑器
            }
            // 自定义精度对话框事件
            Message::OpenCustomPrecisionDialog => {
                // 触发外部对话框窗口（通过 Core 事件）
                lumino_core::event::emit(lumino_core::event::Event::Window(
                    lumino_core::event::window::Event::OpenCustomPrecisionDialog,
                ));
            }
            Message::CloseCustomPrecisionDialog => {
                // 在对话框窗口模式下，触发关闭窗口事件
                if self.is_dialog_window {
                    lumino_core::event::emit(lumino_core::event::Event::Window(
                        lumino_core::event::window::Event::CloseCustomPrecisionDialog,
                    ));
                }
                self.toolbar.custom_precision_dialog.is_open = false;
            }
            Message::ConfirmCustomPrecision => {
                // 确认自定义精度，计算并设置结果
                let tuplet_count = self.toolbar.custom_precision_dialog.tuplet_count.clone();
                let note_value = self.toolbar.custom_precision_dialog.note_value.clone();

                // 设置对话框结果（供独立窗口模式使用）
                self.dialog_result = Some(crate::host::DialogResult::CustomPrecision {
                    numerator: tuplet_count,
                    denominator: note_value,
                });

                // 同时在主窗口应用（兼容模式）
                if let Some(ticks) = self.toolbar.custom_precision_dialog.calculate_ticks(self.editor.state.ppq) {
                    self.toolbar.note_precision = toolbar::NotePrecision::Custom;
                    self.editor.state.snap_precision = ticks;
                    self.editor.state.default_note_length = ticks;
                    tracing::debug!("Root: 自定义精度应用为 {} ticks", ticks);
                }
                self.toolbar.custom_precision_dialog.is_open = false;
            }
            Message::CustomPrecisionNumeratorChanged(value) => {
                // 只接受数字输入（已废弃，保留兼容性）
                if value.chars().all(|c| c.is_ascii_digit()) || value.is_empty() {
                    self.toolbar.custom_precision_dialog.tuplet_count = value;
                }
            }
            Message::CustomPrecisionDenominatorChanged(value) => {
                // 只接受数字输入（已废弃，保留兼容性）
                if value.chars().all(|c| c.is_ascii_digit()) || value.is_empty() {
                    self.toolbar.custom_precision_dialog.note_value = value;
                }
            }
            Message::CustomPrecisionTupletCountChanged(value) => {
                if value.chars().all(|c| c.is_ascii_digit()) || value.is_empty() {
                    self.toolbar.custom_precision_dialog.tuplet_count = value;
                }
            }
            Message::CustomPrecisionTupletTypeChanged(value) => {
                self.toolbar.custom_precision_dialog.tuplet_type = value;
                self.toolbar.custom_precision_dialog.tuplet_count = value.value().to_string();
            }
            Message::CustomPrecisionDotTypeChanged(value) => {
                self.toolbar.custom_precision_dialog.dot_type = value;
            }
            Message::CustomPrecisionNoteValueChanged(value) => {
                if value.chars().all(|c| c.is_ascii_digit()) || value.is_empty() {
                    self.toolbar.custom_precision_dialog.note_value = value;
                }
            }
            Message::CustomPrecisionDivisorChanged(value) => {
                if value.chars().all(|c| c.is_ascii_digit()) || value.is_empty() {
                    self.toolbar.custom_precision_dialog.divisor = value;
                }
            }

        }
    }

    pub fn theme(&self) -> Theme {
        self.window.theme.clone()
    }

    pub fn settings(&self) -> &settings::SettingsPanel {
        &self.settings
    }

    pub fn view(&self) -> Element<'_> {
        if self.is_progress_window {
            // 进度窗口只显示进度
            // 默认显示初始化状态，避免窗口空白
            let (msg, progress) = self
                .progress
                .as_ref()
                .map(|(m, p)| (m.as_str(), *p))
                .unwrap_or(("正在初始化...", 0.0));

            container(
                column![
                    text("处理中...")
                        .size(24)
                        .style(|theme: &Theme| text::Style {
                            color: Some(theme.extended_palette().background.neutral.text),
                        }),
                    text(msg).size(16).style(|theme: &Theme| text::Style {
                        color: Some(theme.extended_palette().background.neutral.text),
                    }),
                    progress_bar(0.0..=1.0, progress as f32),
                ]
                .spacing(20)
                .align_x(iced_core::Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(30)
            .style(|theme: &Theme| container::Style {
                background: Some(iced_core::Background::Color(theme.palette().background)),
                ..Default::default()
            })
            .into()
        } else if self.is_dialog_window {
            // 对话框窗口 - 根据类型显示不同内容
            match self.dialog_type {
                DialogType::Collaboration => self.view_collaboration_dialog(),
                _ => self.view_custom_precision_dialog(),
            }
        } else {
            // 主窗口
            let is_settings_route = self.sidebar.is_settings_route();

            // 左侧栏（包含图标栏和音轨面板）
            let left_bar = self.sidebar.view(&self.window);

            // 主内容区域（工具栏 + 编辑器/设置）
            let main_area: Element<'_> = if is_settings_route {
                // 设置路由激活时显示设置界面
                settings::view(&self.settings, &self.window)
            } else {
                // 默认显示工具栏 + 编辑器
                column![
                    self.toolbar.view(&self.window),
                    self.editor.view(
                        Message::ScrollbarScrolled,
                        Message::ScrollbarScrolledY,
                        |zoom, fixed_ratio| Message::ZoomXChanged { zoom, fixed_ratio },
                        |zoom, fixed_ratio| Message::ZoomYChanged { zoom, fixed_ratio },
                    )
                ]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            };

            let main_content = column![
                self.titlebar.view(&self.window),
                row![left_bar, main_area].height(Length::Fill),
                self.statusbar.view(),
            ];

            let content = container(main_content)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_theme: &Theme| container::Style {
                    background: Some(iced_core::Background::Color(iced_core::Color::TRANSPARENT)),
                    ..Default::default()
                });

            content.into()
        }
    }

    /// 渲染自定义精度对话框（自定义贴合）
    fn view_custom_precision_dialog(&self) -> Element<'_> {
        use iced_widget::{button, column, container, pick_list, row, space, text, text_input};

        let palette = self.window.theme.extended_palette();
        let dialog = &self.toolbar.custom_precision_dialog;

        // 输入框样式
        let input_style = move |_theme: &Theme| container::Style {
            background: Some(palette.background.weak.color.into()),
            border: iced_core::Border {
                radius: 4.0.into(),
                width: 1.0,
                color: palette.background.strong.color,
            },
            ..Default::default()
        };

        // 第一行：三连音数量 + 符点下拉 + 分音符 + "分音符"
        // 当符点类型为（无）时，禁用三连音数量输入框
        let is_tuplet_disabled = dialog.dot_type == crate::toolbar::DotType::None;
        
        let first_row = row![
            // 三连音数量输入框
            container(
                text_input("", &dialog.tuplet_count)
                    .on_input_maybe(if is_tuplet_disabled {
                        None
                    } else {
                        Some(Message::CustomPrecisionTupletCountChanged)
                    })
                    .padding([6, 10])
                    .width(Length::Fixed(50.0))
            )
            .width(Length::Fixed(50.0))
            .style(input_style),
            space().width(8),
            // 符点类型下拉框
            pick_list(
                crate::toolbar::DotType::all(),
                Some(dialog.dot_type),
                Message::CustomPrecisionDotTypeChanged,
            )
            .padding([6, 8])
            .width(Length::Fixed(100.0)),
            space().width(8),
            // 分音符值输入框
            container(
                text_input("", &dialog.note_value)
                    .on_input(Message::CustomPrecisionNoteValueChanged)
                    .padding([6, 10])
                    .width(Length::Fixed(50.0))
            )
            .width(Length::Fixed(50.0))
            .style(input_style),
            space().width(8),
            // "分音符" 标签
            text("分音符").size(14).style(move |_theme: &Theme| text::Style {
                color: Some(palette.background.neutral.text),
            }),
        ]
        .align_y(iced_core::Alignment::Center);

        // 第二行："除以" + 除数输入框
        let second_row = row![
            text("除以").size(14).style(move |_theme: &Theme| text::Style {
                color: Some(palette.background.neutral.text),
            }),
            space().width(50),
            container(
                text_input("", &dialog.divisor)
                    .on_input(Message::CustomPrecisionDivisorChanged)
                    .padding([6, 10])
                    .width(Length::Fixed(50.0))
            )
            .width(Length::Fixed(50.0))
            .style(input_style),
        ]
        .align_y(iced_core::Alignment::Center);

        // 左侧输入区域
        let input_area = column![
            first_row,
            space().height(20),
            second_row,
        ]
        .width(Length::Fixed(320.0))
        .align_x(iced_core::Alignment::Start);

        // 右侧按钮区域（垂直排列）
        let buttons = column![
            button(text("确定").size(14))
                .on_press(Message::ConfirmCustomPrecision)
                .padding([8, 32])
                .width(Length::Fixed(100.0))
                .style(move |_theme: &Theme, status| {
                    let bg = match status {
                        button::Status::Hovered => palette.primary.strong.color,
                        _ => palette.primary.base.color,
                    };
                    button::Style {
                        background: Some(bg.into()),
                        text_color: iced_core::Color::WHITE,
                        border: iced_core::Border {
                            radius: 4.0.into(),
                            width: 0.0,
                            color: iced_core::Color::TRANSPARENT,
                        },
                        snap: false,
                        shadow: Default::default(),
                    }
                }),
            space().height(12),
            button(text("取消").size(14))
                .on_press(Message::CloseCustomPrecisionDialog)
                .padding([8, 32])
                .width(Length::Fixed(100.0))
                .style(move |_theme: &Theme, status| {
                    let bg = match status {
                        button::Status::Hovered => palette.background.strong.color,
                        _ => palette.background.weak.color,
                    };
                    button::Style {
                        background: Some(bg.into()),
                        text_color: palette.background.neutral.text,
                        border: iced_core::Border {
                            radius: 4.0.into(),
                            width: 0.0,
                            color: iced_core::Color::TRANSPARENT,
                        },
                        shadow: Default::default(),
                        snap: false,
                    }
                }),
        ]
        .align_x(iced_core::Alignment::Center);

        // 主内容区域：左侧输入 + 右侧按钮
        let main_content = row![
            input_area,
            space().width(Length::Fixed(20.0)),
            buttons,
        ]
        .align_y(iced_core::Alignment::Center);

        let dialog_content = container(main_content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(24)
            .style(move |_theme: &Theme| {
                container::Style::default()
                    .background(palette.background.base.color)
            });

        dialog_content.into()
    }

    /// 渲染协作对话框
    fn view_collaboration_dialog(&self) -> Element<'_> {
        use iced_widget::{button, column, container, row, space, text, text_input};

        let palette = self.window.theme.extended_palette();
        let dialog = &self.collaboration_dialog;

        // 标题
        let title = text("多人协作")
            .size(20)
            .style(move |_theme: &Theme| text::Style {
                color: Some(palette.background.neutral.text),
            });

        // 根据当前视图状态显示不同内容
        let content: Element<'_> = match dialog.view_state {
            CollaborationViewState::Connect => {
                // 连接服务器界面
                let host_input = text_input("服务器地址", &dialog.server_host)
                    .on_input(Message::CollaborationHostChanged)
                    .padding(8)
                    .width(Length::Fill);

                let port_input = text_input("端口", &dialog.server_port)
                    .on_input(Message::CollaborationPortChanged)
                    .padding(8)
                    .width(Length::Fixed(80.0));

                let username_input = text_input("用户", &dialog.username)
                    .on_input(Message::CollaborationUsernameChanged)
                    .padding(8)
                    .width(Length::Fill);

                let invite_input = text_input("邀请码（可选）", &dialog.invite_code)
                    .on_input(Message::CollaborationInviteCodeChanged)
                    .padding(8)
                    .width(Length::Fill);

                let connect_button = button(text("连接").size(14))
                    .on_press(Message::CollaborationConnect {
                        host: dialog.server_host.clone(),
                        port: dialog.server_port.parse().unwrap_or(3000),
                        username: dialog.username.clone(),
                        invite_code: if dialog.invite_code.trim().is_empty() { None } else { Some(dialog.invite_code.clone()) },
                    })
                    .padding([8, 24])
                    .style(move |_theme: &Theme, status| {
                        let bg = match status {
                            button::Status::Hovered => palette.primary.strong.color,
                            _ => palette.primary.base.color,
                        };
                        button::Style {
                            background: Some(bg.into()),
                            text_color: iced_core::Color::WHITE,
                            border: iced_core::Border {
                                radius: 4.0.into(),
                                width: 0.0,
                                color: iced_core::Color::TRANSPARENT,
                            },
                            snap: false,
                            shadow: Default::default(),
                        }
                    });

                column![
                    row![host_input, space().width(8), port_input].align_y(iced_core::Alignment::Center),
                    space().height(12),
                    username_input,
                    space().height(12),
                    invite_input,
                    space().height(16),
                    connect_button,
                ]
                .align_x(iced_core::Alignment::Center)
                .into()
            }
            CollaborationViewState::RoomActions => {
                // 创建/加入房间界面
                let room_name_input = text_input("房间名称", &dialog.room_name)
                    .on_input(Message::CollaborationRoomNameChanged)
                    .padding(8)
                    .width(Length::Fill);

                let create_button = button(text("创建房间").size(14))
                    .on_press(Message::CollaborationCreateRoom {
                        name: dialog.room_name.clone(),
                    })
                    .padding([8, 24])
                    .width(Length::Fill)
                    .style(move |_theme: &Theme, status| {
                        let bg = match status {
                            button::Status::Hovered => palette.primary.strong.color,
                            _ => palette.primary.base.color,
                        };
                        button::Style {
                            background: Some(bg.into()),
                            text_color: iced_core::Color::WHITE,
                            border: iced_core::Border {
                                radius: 4.0.into(),
                                width: 0.0,
                                color: iced_core::Color::TRANSPARENT,
                            },
                            snap: false,
                            shadow: Default::default(),
                        }
                    });

                let or_text = text("- 或 -")
                    .size(12)
                    .style(move |_theme: &Theme| text::Style {
                        color: Some(palette.background.neutral.text),
                    });

                let invite_input = text_input("邀请码", &dialog.invite_code)
                    .on_input(Message::CollaborationInviteCodeChanged)
                    .padding(8)
                    .width(Length::Fill);

                let join_button = button(text("加入房间").size(14))
                    .on_press(Message::CollaborationJoinRoom {
                        invite_code: dialog.invite_code.clone(),
                    })
                    .padding([8, 24])
                    .width(Length::Fill)
                    .style(move |_theme: &Theme, status| {
                        let bg = match status {
                            button::Status::Hovered => palette.background.strong.color,
                            _ => palette.background.weak.color,
                        };
                        button::Style {
                            background: Some(bg.into()),
                            text_color: palette.background.neutral.text,
                            border: iced_core::Border {
                                radius: 4.0.into(),
                                width: 0.0,
                                color: iced_core::Color::TRANSPARENT,
                            },
                            snap: false,
                            shadow: Default::default(),
                        }
                    });

                column![
                    room_name_input,
                    space().height(8),
                    create_button,
                    space().height(16),
                    or_text,
                    space().height(16),
                    invite_input,
                    space().height(8),
                    join_button,
                ]
                .align_x(iced_core::Alignment::Center)
                .into()
            }
            CollaborationViewState::InRoom => {
                // 在房间内界面
                let room_info = column![
                    text(format!("房间: {}", dialog.room_name))
                        .size(16)
                        .style(move |_theme: &Theme| text::Style {
                            color: Some(palette.background.neutral.text),
                        }),
                    space().height(8),
                    row![
                        text("邀请码: ")
                            .size(14)
                            .style(move |_theme: &Theme| text::Style {
                                color: Some(palette.background.neutral.text),
                            }),
                        text(&dialog.invite_code)
                            .size(14)
                            .style(move |_theme: &Theme| text::Style {
                                color: Some(palette.primary.base.color),
                            }),
                    ]
                    .align_y(iced_core::Alignment::Center),
                ]
                .align_x(iced_core::Alignment::Center);

                let copy_button = button(text("复制邀请码").size(12))
                    .on_press(Message::CollaborationCopyInviteCode)
                    .padding([6, 16])
                    .style(move |_theme: &Theme, status| {
                        let bg = match status {
                            button::Status::Hovered => palette.background.strong.color,
                            _ => palette.background.weak.color,
                        };
                        button::Style {
                            background: Some(bg.into()),
                            text_color: palette.background.neutral.text,
                            border: iced_core::Border {
                                radius: 4.0.into(),
                                width: 0.0,
                                color: iced_core::Color::TRANSPARENT,
                            },
                            snap: false,
                            shadow: Default::default(),
                        }
                    });

                let disconnect_button = button(text("断开连接").size(14))
                    .on_press(Message::CollaborationDisconnect)
                    .padding([8, 24])
                    .style(move |_theme: &Theme, status| {
                        let bg = match status {
                            button::Status::Hovered => palette.danger.strong.color,
                            _ => palette.danger.base.color,
                        };
                        button::Style {
                            background: Some(bg.into()),
                            text_color: iced_core::Color::WHITE,
                            border: iced_core::Border {
                                radius: 4.0.into(),
                                width: 0.0,
                                color: iced_core::Color::TRANSPARENT,
                            },
                            snap: false,
                            shadow: Default::default(),
                        }
                    });

                column![
                    room_info,
                    space().height(16),
                    copy_button,
                    space().height(24),
                    disconnect_button,
                ]
                .align_x(iced_core::Alignment::Center)
                .into()
            }
        };

        // 关闭按钮
        let close_button = button(text("关闭").size(12))
            .on_press(Message::CloseCollaborationDialog)
            .padding([6, 16])
            .style(move |_theme: &Theme, status| {
                let bg = match status {
                    button::Status::Hovered => palette.background.strong.color,
                    _ => palette.background.weak.color,
                };
                button::Style {
                    background: Some(bg.into()),
                    text_color: palette.background.neutral.text,
                    border: iced_core::Border {
                        radius: 4.0.into(),
                        width: 0.0,
                        color: iced_core::Color::TRANSPARENT,
                    },
                    snap: false,
                    shadow: Default::default(),
                }
            });

        let dialog_content = column![
            row![title, space().width(Length::Fill), close_button].align_y(iced_core::Alignment::Center),
            space().height(20),
            content,
        ]
        .align_x(iced_core::Alignment::Center);

        container(dialog_content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(24)
            .style(move |_theme: &Theme| {
                container::Style::default()
                    .background(palette.background.base.color)
            })
            .into()
    }

    /// 获取当前需要绘制的音符实例
    pub fn get_note_instances(&self) -> Vec<NoteInstance> {
        let sidebar_width = self.sidebar.width() as f32;
        let mut instances = self
            .editor
            .get_note_instances(&self.window.theme, sidebar_width);

        // 添加洋葱皮音符（其他音轨的音符）
        let onion_states = self.sidebar.get_onion_skin_states();
        let onion_instances = self.editor.get_all_onion_skin_instances(&onion_states);
        instances.extend(onion_instances);

        instances
    }

    /// 获取并清空待处理的音频动作
    pub fn take_audio_actions(&mut self) -> Vec<message::AudioAction> {
        self.editor.take_audio_actions()
    }

    /// 更新编辑器鼠标位置
    pub fn update_editor_cursor(&mut self, position: Option<iced_core::Point>) {
        self.editor.update_cursor_position(position);
    }

    /// 更新编辑器 Canvas 偏移量
    pub fn set_editor_canvas_offset(&mut self, offset: iced_core::Point) {
        self.editor.set_canvas_offset(offset);
    }

    /// 设置菜单打开状态（菜单打开时不渲染预览音符）
    pub fn set_menu_open(&mut self, open: bool) {
        self.is_menu_open = open;
    }

    /// 获取当前是否应该渲染预览音符
    pub fn should_render_preview_note(&self) -> bool {
        !self.is_menu_open && !self.is_progress_window
    }

    /// 更新音轨列表（从 MIDI 导入）
    pub fn update_tracks(&mut self, track_infos: &[(usize, Option<String>, u64)]) {
        self.sidebar.update_tracks_from_midi(track_infos);
    }

    /// 设置编辑器总 ticks
    pub fn set_total_ticks(&mut self, total_ticks: f32) {
        self.editor.state.total_ticks = total_ticks as u32;
        self.editor.max_scroll_x = total_ticks * self.editor.state.zoom_x;
    }

    /// 加载音符到编辑器
    pub fn load_notes(&mut self, notes: &[(f32, u8, f32)]) {
        self.editor.notes.clear();
        for (tick, key, length) in notes {
            // MIDI key (0-127) 映射到编辑器 key (0-127，反转顺序)
            let editor_key = *key as u16;
            self.editor
                .notes
                .push(Note::new(*tick, editor_key, *length));
        }
        // 清除网格缓存以强制重绘
        self.editor.grid_cache.clear();
    }

    /// 设置当前音轨
    pub fn editor_ref(&self) -> &editor::Editor {
        &self.editor
    }

    pub fn set_current_track(&mut self, track_idx: usize) {
        self.sidebar.set_selected_track(track_idx);
        // 同时更新编辑器的当前音轨（用于无 MIDI 文件时的多音轨编辑）
        self.editor.switch_to_track(track_idx);
    }

    /// 加载指定音轨的音符到编辑器（用于 MIDI 文件）
    /// 这会同时更新当前显示的音符和音轨存储，以便洋葱皮能显示
    pub fn load_track_notes(&mut self, track_idx: usize, notes: &[(f32, u8, f32)]) {
        tracing::debug!(
            "Root::load_track_notes: track_idx={}, notes_count={}",
            track_idx,
            notes.len()
        );

        // 清空当前音符并加载新音符
        self.editor.notes.clear();
        let mut track_notes = Vec::with_capacity(notes.len());

        for (tick, key, length) in notes {
            let editor_key = *key as u16;
            let note = Note::new(*tick, editor_key, *length);
            self.editor.notes.push(note.clone());
            track_notes.push(note);
        }

        // 保存到 track_notes，供洋葱皮使用
        if !track_notes.is_empty() {
            self.editor.track_notes.insert(track_idx, track_notes);
            tracing::debug!(
                "Root::load_track_notes: saved {} notes to track_notes[{}]",
                notes.len(),
                track_idx
            );
        }

        // 更新当前音轨索引
        self.editor.current_track = track_idx;

        // 清除网格缓存以强制重绘
        self.editor.grid_cache.clear();
    }

    /// 设置自定义精度对话框是否打开
    pub fn set_custom_precision_dialog_open(&mut self, open: bool) {
        self.toolbar.custom_precision_dialog.is_open = open;
        if open {
            self.dialog_type = DialogType::CustomPrecision;
        }
    }

    /// 获取并清空对话框结果
    pub fn take_dialog_result(&mut self) -> Option<crate::host::DialogResult> {
        self.dialog_result.take()
    }

    /// 设置自定义精度值
    pub fn set_custom_precision(&mut self, ticks: f32) {
        self.editor.state.snap_precision = ticks;
        self.editor.state.default_note_length = ticks;
        self.toolbar.note_precision = toolbar::NotePrecision::Custom;
        tracing::info!("自定义精度已设置为 {} ticks", ticks);
    }

    /// 设置协作对话框是否打开
    pub fn set_collaboration_dialog_open(&mut self, open: bool) {
        self.collaboration_dialog.is_open = open;
        if open {
            self.dialog_type = DialogType::Collaboration;
            self.collaboration_dialog.view_state = CollaborationViewState::Connect;
        }
        tracing::info!("协作对话框状态: {}", open);
    }

    /// 设置协作视图状态
    pub fn set_collaboration_view_state(
        &mut self,
        state: CollaborationViewState,
        invite_code: Option<String>,
        room_name: Option<String>,
    ) {
        self.collaboration_dialog.view_state = state;
        if let Some(code) = invite_code {
            self.collaboration_dialog.invite_code = code;
        }
        if let Some(name) = room_name {
            self.collaboration_dialog.room_name = name;
        }
        match state {
            CollaborationViewState::Connect => {
                self.collaboration_dialog.connection_status = "未连接".to_string();
            }
            CollaborationViewState::RoomActions => {
                self.collaboration_dialog.connection_status = "已连接，请创建或加入房间".to_string();
            }
            CollaborationViewState::InRoom => {
                self.collaboration_dialog.connection_status = format!(
                    "房间: {} | 邀请码: {}",
                    self.collaboration_dialog.room_name,
                    self.collaboration_dialog.invite_code
                );
            }
        }
        tracing::info!("协作视图状态已更新: {:?}", state);
    }
}
