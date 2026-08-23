//! 钢琴卷帘编辑区主视图 — view_main 与素材删除确认对话框

use iced_core::Length;
use iced_widget::{column, container, row};

use super::super::right_content;
use crate::message;
use crate::right_sidebar;
use crate::root::Root;
use crate::{Element, Theme};

impl Root {
    /// 渲染主窗口
    pub(crate) fn view_main(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_main");

        // 全屏瀑布流播放器：仅渲染瀑布流 + 键盘，剥离钢琴卷帘全部 UI
        // （工具栏 / 力度面板 / 状态栏 / 卷帘画布 / 右侧栏 / 左侧轨道列表面板）。
        // 仅保留全局导航栏（标题栏含模式切换退出入口、左侧 48px 路由栏），
        // 二者属应用级导航而非钢琴卷帘界面内容。
        if self.state.current_mode == crate::titlebar::mode_toggle::AppMode::Waterfall {
            return self.view_waterfall_fullscreen();
        }

        let is_arrangement_route = self.sidebar.is_arrangement_route();

        // 左侧栏（包含图标栏和音轨面板）
        puffin::profile_scope!("root_view_sidebar");
        let ppq = self.editor.editor_state.view.ppq;
        let left_bar = self.sidebar.view(
            &self.window,
            self.settings.display.language,
            self.state.current_mode,
            self.toolbar.note_precision.as_ticks(ppq),
        );

        // 右侧内容区域（工具栏 + 编辑器 + 力度面板 / 全屏瀑布流播放器）
        puffin::profile_scope!("root_view_right_content");
        let right_content: Element<'_> = if is_arrangement_route {
            // 音轨总览模式：使用 wgpu 原生渲染
            right_content::wrap_right_content(self, false, true, |available_width| {
                self.view_arrangement(available_width)
            })
        } else if self.sidebar.audio_export_visible {
            // 音频渲染面板（在主界面钢琴卷帘区域显示）
            self.view_audio_export_panel()
        } else if self.sidebar.video_export_visible {
            // 视频渲染面板（在主界面钢琴卷帘区域显示）
            self.view_video_export_panel()
        } else if !self.sidebar.piano_roll_visible {
            // 渲染器入口面板（首级面板）：当 Renderer 分组激活且未进入子面板时，展示视频剪辑窗口
            // 对标钢琴卷帘分组的 File/Automation 子面板逻辑，进入/退出由 GroupId::Renderer 状态驱动
            if self.sidebar.active_group == Some(lumino_ui_core::sidebar_event::GroupId::Renderer) {
                self.view_renderer_panel()
            } else {
                // 钢琴卷帘已关闭：显示空白区域
                container(
                    iced_widget::column![]
                        .width(Length::Fill)
                        .height(Length::Fill),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|theme: &Theme| container::Style {
                    background: Some(iced_core::Background::Color(theme.palette().background)),
                    ..Default::default()
                })
                .into()
            }
        } else {
            // 钢琴卷帘编辑区 —— 右侧栏唯一渲染位置。
            // 右侧栏跟随钢琴卷帘 UI 显隐（right_sidebar_visible 收口）：
            // 离开钢琴卷帘（工程走带/瀑布流/导出面板/卷帘关闭）时由上方
            // 各分支接管，不渲染右侧栏。
            // 纵向卷帘：底部横向钢琴键盘 + 水平时间 / 垂直音高网格，复用同款主题与右/底边栏
            let has_selection = self.editor.selected_notes_count() > 0;
            if self.sidebar.is_vertical_roll() {
                right_content::wrap_right_content(
                    self,
                    has_selection,
                    false,
                    move |available_width| {
                        let velocity_panel = if self.sidebar.automation_panel_visible {
                            self.editor.velocity_panel.view(
                                &self.editor,
                                self.visual.velocity_panel_height,
                                self.settings.display.language,
                            )
                        } else {
                            iced_widget::Space::new().height(0).into()
                        };
                        let editor_view = self.editor.view_vertical(
                            message::Message::ScrollbarScrolled,
                            message::Message::ScrollbarScrolledY,
                            |zoom, fixed_ratio| message::Message::ZoomXChanged {
                                zoom,
                                fixed_ratio,
                            },
                            |zoom, fixed_ratio| message::Message::ZoomYChanged {
                                zoom,
                                fixed_ratio,
                            },
                        );
                        let perf_ctx = crate::toolbar::ToolbarPerfContext {
                            playback_tick: self.editor.playback_position,
                            ppq: self.editor.editor_state.view.ppq,
                            tempo_points: &self.editor.editor_state.data.tempo_points,
                        };
                        let toolbar = self.toolbar.toolbar_view(
                            &self.window,
                            has_selection,
                            self.settings.display.language,
                            &perf_ctx,
                            available_width,
                            false,
                        );
                        let right_bar = if self.right_sidebar_visible() {
                            right_sidebar::view::view(
                                &self.right_sidebar,
                                &self.window,
                                self.settings.display.language,
                            )
                        } else {
                            iced_widget::Space::new().into()
                        };
                        column![
                            toolbar,
                            row![
                                column![
                                    container(editor_view).height(Length::Fill),
                                    velocity_panel,
                                ]
                                .height(Length::Fill),
                                right_bar,
                            ]
                            .height(Length::Fill),
                        ]
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .into()
                    },
                )
            } else {
                right_content::wrap_right_content(
                    self,
                    has_selection,
                    false,
                    move |available_width| {
                        let velocity_panel = if self.sidebar.automation_panel_visible {
                            self.editor.velocity_panel.view(
                                &self.editor,
                                self.visual.velocity_panel_height,
                                self.settings.display.language,
                            )
                        } else {
                            iced_widget::Space::new().height(0).into()
                        };
                        let editor_view = self.editor.view(
                            message::Message::ScrollbarScrolled,
                            message::Message::ScrollbarScrolledY,
                            |zoom, fixed_ratio| message::Message::ZoomXChanged {
                                zoom,
                                fixed_ratio,
                            },
                            |zoom, fixed_ratio| message::Message::ZoomYChanged {
                                zoom,
                                fixed_ratio,
                            },
                        );
                        let perf_ctx = crate::toolbar::ToolbarPerfContext {
                            playback_tick: self.editor.playback_position,
                            ppq: self.editor.editor_state.view.ppq,
                            tempo_points: &self.editor.editor_state.data.tempo_points,
                        };
                        let toolbar = self.toolbar.toolbar_view(
                            &self.window,
                            has_selection,
                            self.settings.display.language,
                            &perf_ctx,
                            available_width,
                            false,
                        );
                        // 右侧栏渲染条件收口：仅钢琴卷帘编辑区渲染（防御性兜底，
                        // 正常情况下该分支即满足 right_sidebar_visible）
                        let right_bar = if self.right_sidebar_visible() {
                            right_sidebar::view::view(
                                &self.right_sidebar,
                                &self.window,
                                self.settings.display.language,
                            )
                        } else {
                            iced_widget::Space::new().into()
                        };
                        column![
                            toolbar,
                            row![
                                column![
                                    container(editor_view).height(Length::Fill),
                                    velocity_panel,
                                ]
                                .height(Length::Fill),
                                right_bar,
                            ]
                            .height(Length::Fill),
                        ]
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .into()
                    },
                )
            }
        };

        puffin::profile_scope!("root_view_main_content");
        let main_content = if cfg!(target_os = "macos") {
            column![
                row![left_bar, right_content].height(Length::Fill),
                self.view_status_section(),
            ]
        } else {
            // 导出为素材的启用条件：卷帘选中音符 或 走带视图跨音轨框选
            let export_material_enabled = self.editor.selected_notes_count() > 0
                || !self.editor.editor_state.data.arrange_selection.is_empty();
            column![
                self.titlebar.view(
                    &self.window,
                    self.settings.synth.use_native_titlebar,
                    self.state.current_mode,
                    self.state.toggle_animation.position,
                    self.settings.display.language,
                    export_material_enabled,
                ),
                row![left_bar, right_content].height(Length::Fill),
                self.view_status_section(),
            ]
        };

        // 叠加层（自下而上）：Toast 通知层（右下角）→ 素材删除确认对话框（覆盖层样式）
        let mut stack = iced_widget::Stack::new()
            .push(main_content)
            .width(Length::Fill)
            .height(Length::Fill);
        if let Some(toast_overlay) = self.toast.view(&self.window.theme) {
            stack = stack.push(toast_overlay);
        }
        if let Some(dialog) = self.view_material_delete_dialog() {
            stack = stack.push(dialog);
        }
        // 混音台入口按钮（左下悬浮，点亮表示面板打开）
        stack = stack.push(crate::root::mixer_panel::view_mixer_entry(self));
        // 混音台浮动面板（非阻塞覆盖层，打开时叠加于最上层）
        if let Some(panel) = crate::root::mixer_panel::view_mixer_panel(self) {
            stack = stack.push(panel);
        }
        stack.into()
    }

    /// 素材删除确认对话框（主窗口覆盖层：全屏遮罩 + 居中卡片）
    ///
    /// 由 `right_sidebar.materials.pending_delete` 驱动；无待确认项时返回 None。
    /// 素材名优先取快照（列表刷新后仍可正确展示），回退到列表条目。
    pub(crate) fn view_material_delete_dialog(&self) -> Option<Element<'static>> {
        let index = self.right_sidebar.materials.pending_delete?;
        let name = self
            .right_sidebar
            .materials
            .pending_delete_name
            .clone()
            .or_else(|| {
                self.right_sidebar
                    .materials
                    .entries
                    .get(index)
                    .map(|e| e.name.clone())
            })
            .unwrap_or_default();
        Some(crate::right_sidebar::material_delete_dialog::view(
            name,
            index,
            self.settings.display.language,
        ))
    }
}
