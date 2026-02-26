use iced_core::Length;
use iced_widget::{column, container, mouse_area, progress_bar, row, space, text};
use lumino_gfx::NoteInstance;

use crate::{editor, editor::note::Note, message, sidebar, statusbar, titlebar, window};

pub type Message = message::Message;
pub type Theme = iced_core::Theme;
pub type Renderer = iced_wgpu::Renderer;
pub type Element<'a> = iced_core::Element<'a, Message, Theme, Renderer>;

pub struct Root {
    sidebar: sidebar::Sidebar,
    titlebar: titlebar::Titlebar,
    statusbar: statusbar::StatusBar,
    pub editor: editor::Editor,
    window: window::Window,
    progress: Option<(String, f64)>,
    is_progress_window: bool,
    /// 是否有菜单/下拉框打开（打开时不渲染预览音符）
    is_menu_open: bool,
}

impl Root {
    pub fn new(theme: &str) -> Self {
        Self {
            sidebar: sidebar::Sidebar::new(),
            titlebar: titlebar::Titlebar::new(),
            statusbar: statusbar::StatusBar::new(),
            editor: editor::Editor::new(),
            window: window::Window::new(theme),
            progress: None,
            is_progress_window: false,
            is_menu_open: false,
        }
    }

    pub fn new_progress(theme: &str) -> Self {
        Self {
            sidebar: sidebar::Sidebar::new(),
            titlebar: titlebar::Titlebar::new(),
            statusbar: statusbar::StatusBar::new(),
            editor: editor::Editor::new(),
            window: window::Window::new(theme),
            progress: None,
            is_progress_window: true,
            is_menu_open: false,
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
            // 显式丢弃它
            Message::Null => (),
        }
    }

    pub fn theme(&self) -> Theme {
        self.window.theme.clone()
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
        } else {
            // 主窗口
            let main_content = column![
                self.titlebar.view(&self.window),
                row![
                    self.sidebar.view(&self.window),
                    self.editor.view(
                        Message::ScrollbarScrolled,
                        Message::ScrollbarScrolledY,
                        |zoom, fixed_ratio| Message::ZoomXChanged { zoom, fixed_ratio },
                        |zoom, fixed_ratio| Message::ZoomYChanged { zoom, fixed_ratio }
                    )
                ],
                self.statusbar.view(),
            ];

            let content = container(main_content)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_theme: &Theme| container::Style {
                    background: Some(iced_core::Background::Color(iced_core::Color::TRANSPARENT)),
                    ..Default::default()
                });

            // 如果菜单打开，添加一个透明的覆盖层来捕获点击事件并关闭菜单
            content.into()
        }
    }

    /// 获取当前需要绘制的音符实例
    pub fn get_note_instances(&self) -> Vec<NoteInstance> {
        let sidebar_width = self.sidebar.width() as f32;
        self.editor
            .get_note_instances(&self.window.theme, sidebar_width)
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
            self.editor.notes.push(Note::new(*tick, editor_key, *length));
        }
        // 清除网格缓存以强制重绘
        self.editor.grid_cache.clear();
    }

    /// 设置当前音轨
    pub fn set_current_track(&mut self, track_idx: usize) {
        self.sidebar.set_selected_track(track_idx);
    }
}
