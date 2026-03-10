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
                    tracing::debug!("Root: 发射音轨选择事件，音轨 {}", track_idx);
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
                self.toolbar.update(event);
            }
            // 显式丢弃它
            Message::Null => (),
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
            // 对话框窗口 - 显示自定义精度对话框内容
            self.view_custom_precision_dialog()
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
}
