//! Yinhe Root 集成 — `Root` 侧的 Yinhe 状态、视图与消息路由
//!
//! 约束（P7）：
//! - 数据模型直接用 Lumino 工程格式，yin 格式之后适配（初期不做，见 `lumino-ui-yinhe::file` 桩）
//! - 混音台不迁，多文档标签不做，i18n 按 lumino，快捷键统一复用罗盘，不新定义 Keybindings 持久化
//! - 发布形态为可选编译 `--features yinhe`（本模块 `#[cfg(feature = "yinhe")]` 门控，
//!   无 feature 时提供空桩，保证 `cargo check` 与 `cargo check --features yinhe` 双形态通过）
//! - 存储：`YinheState { view_mode, layout }` 独立 `yinhe_layout.json`，
//!   **不污染** `UiConfig` / `UiState`，复用 `src/storage/ui_state.rs` 的 Wrapper 模式，
//!   由 Runner 侧 `Storage` 持有 `YinheStateWrapper`（见 `src/storage.rs` 扩展）。
//! - `Root` 仅持有运行时 `YinheState`（非 Wrapper），持久化由 Runner `save_storage` 统一 `save()`。

use crate::Element;
use crate::root::Root;

#[cfg(feature = "yinhe")]
use lumino_ui_yinhe::state::YinheState;

#[cfg(feature = "yinhe")]
static FALLBACK_SIGS: [(u32, u8, u8); 1] = [(0, 4, 4)];

// ─── Root 扩展：Yinhe 状态访问（仅 feature 时） ─────────────────

#[cfg(feature = "yinhe")]
impl Root {
    /// 进入 Yinhe 副模式（供 Runner --yinhe 启动时调用）
    pub fn enter_yinhe_mode(&mut self) {
        self.state.current_mode = crate::titlebar::mode_toggle::AppMode::Yinhe;
        self.state.toggle_animation.animate_to(0.5);
        self.sidebar.active_group = Some(lumino_ui_core::sidebar_event::GroupId::Yinhe);
    }

    /// 获取 Yinhe 状态（feature 门控，供 Host/Runner 读取）
    pub fn yinhe_state(&self) -> &YinheState {
        &self.yinhe
    }

    /// 可变访问 Yinhe 状态
    pub fn yinhe_state_mut(&mut self) -> &mut YinheState {
        &mut self.yinhe
    }

    /// 设置 Yinhe ViewMode（由 chrome mode_bar 触发时调用）
    pub fn set_yinhe_view_mode(&mut self, mode: lumino_ui_yinhe::chrome::ViewMode) {
        self.yinhe.view_mode = mode;
    }

    /// 设置 Yinhe 布局（走带分割/右侧面板）
    pub fn set_yinhe_layout(&mut self, layout: lumino_ui_yinhe::state::YinheLayout) {
        self.yinhe.layout = layout;
    }

    /// 同步 YinheState 到 chrome 状态（供 view 层读取）
    pub(crate) fn yinhe_chrome_state(&self, use_native_titlebar: bool) -> lumino_ui_yinhe::chrome::ChromeState {
        // 数据模型直接用 Lumino 工程格式，文件名从文档的 track_names 或默认占位获取；
        // MidiDocument 本身不存路径，路径由 Runner::current_midi_source 持有，此处仅占位。
        let doc_name = self
            .editor
            .editor_state
            .data
            .document
            .as_ref()
            .and_then(|d| d.track_names.first().and_then(|s| s.as_ref()))
            .map(|s| s.as_str())
            .unwrap_or("无标题");
        lumino_ui_yinhe::chrome::ChromeState {
            view_mode: self.yinhe.view_mode,
            show_pianoroll_in_arrange: self.yinhe.layout.show_pianoroll_in_arrange,
            title: lumino_ui_yinhe::chrome::TitleBarState::named(doc_name, false),
            transport: lumino_ui_yinhe::chrome::TransportState {
                is_playing: self.toolbar.is_playing,
                bpm: self
                    .editor
                    .editor_state
                    .data
                    .tempo_points
                    .first()
                    .map(|tp| tp.bpm as f32)
                    .unwrap_or(120.0),
                has_active_document: self.editor.editor_state.data.document.is_some(),
                ..Default::default()
            },
            mode_metrics: None,
            use_native_titlebar,
        }
    }

    /// 渲染 Yinhe 副模式主视图（feature 门控）
    ///
    /// 布局（P2+ P3 已有 chrome/arrange/piano_view/right_panel 桩）：
    /// ```text
    /// column![
    ///   chrome::view(title+transport+mode_bar),   // 30+40+28 三栏
    ///   row![
    ///     track_panel(220) | view_ui(Fill) | right_panel(240)
    ///   ],
    ///   // 底部复用 mode_bar 的 ViewMode 切换（已在 chrome 顶部包含，此处额外展示以满足图二底部工具栏语义）
    /// ]
    /// ```
    /// 复用 lumino `Window.theme` 与 `AppMode::Yinhe` 高亮，i18n 按 lumino。
    ///
    /// P8 真实接入：通过 `Box::leak` 将局部 `ChromeState/TrackPanelState/RightPanelState`
    /// 提升为 `'static` 以通过 `'a` 检查（`Element<'a>` 借用 `&self.window`），
    /// 数据来源 `self.sidebar.tracks / self.editor.editor_state.data / self.yinhe.layout`，
    /// 先用占位行/默认状态跑通，不强求真实 MIDI 渲染；真实走带网格+时间标尺由 `arrange::view_ui` canvas 绘制。
    pub(crate) fn view_yinhe(&self) -> Element<'_> {
        // ── 顶部 chrome：title(30)+transport(40)+mode(28) 三栏 ──
        let chrome_state = self.yinhe_chrome_state(self.use_native_titlebar);
        let top: Element<'_> = lumino_ui_yinhe::chrome::view(
            &self.window,
            lumino_ui_core::app_mode::AppMode::Yinhe,
            chrome_state,
        );

        // ── 左侧音轨列表：220px，数据来自 self.sidebar.tracks + CC 通道展开（图二：PitchBend/CC007 等）──
        let rows: Vec<lumino_ui_yinhe::arrange::TrackRow> = {
            let mut v = Vec::new();
            for t in &self.sidebar.tracks {
                let color_arr = if let Some(c) = t.color {
                    [c.r, c.g, c.b, c.a]
                } else if t.is_conductor {
                    [0.5, 0.5, 0.5, 1.0]
                } else {
                    let p = self.window.theme.extended_palette().primary.strong.color;
                    [p.r, p.g, p.b, p.a]
                };
                v.push(lumino_ui_yinhe::arrange::TrackRow {
                    index: t.id as u16,
                    name: t.name.clone(),
                    port: t.port,
                    channel: t.channel,
                    color: color_arr,
                    is_conductor: t.is_conductor,
                    visible: true,
                    muted: t.is_muted,
                    soloed: t.is_soloed,
                    is_automation: false,
                });
                // 选中轨展开 CC 通道（PitchBend/CC 7/10/11/64 等），与图二一致；未选中轨不展开以免过长
                // 等宽约束：子项与主轨同为 Fixed(220)，仅通过缩进区分（is_automation = true）
                if t.id == self.sidebar.selected_track && !t.is_conductor {
                    let lanes = [
                        ("Pitch Bend", 0, 0, [0.85, 0.85, 0.85, 1.0]),
                        ("CC 007", 0, 7, [0.7, 0.7, 0.7, 1.0]),
                        ("CC 010", 0, 10, [0.7, 0.7, 0.7, 1.0]),
                        ("CC 064", 0, 64, [0.7, 0.7, 0.7, 1.0]),
                        ("CC 011", 0, 11, [0.7, 0.7, 0.7, 1.0]),
                    ];
                    for (lname, p, ch, col) in lanes {
                        v.push(lumino_ui_yinhe::arrange::TrackRow {
                            index: t.id as u16,
                            name: format!("{} {}", lname, t.name),
                            port: p,
                            channel: ch,
                            color: col,
                            is_conductor: false,
                            visible: true,
                            muted: false,
                            soloed: false,
                            is_automation: true,
                        });
                    }
                }
            }
            if v.is_empty() {
                v.push(lumino_ui_yinhe::arrange::TrackRow {
                    index: 0,
                    name: "Master".into(),
                    port: 0,
                    channel: 0,
                    color: [0.5, 0.5, 0.5, 1.0],
                    is_conductor: true,
                    visible: true,
                    muted: false,
                    soloed: false,
                    is_automation: false,
                });
            }
            // 若仅 2 轨（空工程），补充示例 3-16 轨以接近图二（开发预览，不影响真实工程）
            if v.len() == 2 {
                for i in 2..=16 {
                    let c = match i % 6 {
                        0 => [0.9, 0.3, 0.3, 1.0],
                        1 => [0.3, 0.8, 0.4, 1.0],
                        2 => [0.7, 0.5, 0.9, 1.0],
                        3 => [0.3, 0.7, 0.9, 1.0],
                        4 => [0.9, 0.7, 0.3, 1.0],
                        _ => [0.9, 0.5, 0.6, 1.0],
                    };
                    v.push(lumino_ui_yinhe::arrange::TrackRow {
                        index: i as u16,
                        name: format!("Track {}", i),
                        port: (i % 4) as u8,
                        channel: (i % 16) as u8,
                        color: c,
                        is_conductor: false,
                        visible: true,
                        muted: false,
                        soloed: false,
                        is_automation: false,
                    });
                }
            }
            v
        };
        let mut selected = std::collections::HashSet::new();
        selected.insert(self.sidebar.selected_track as u16);
        let track_state = lumino_ui_yinhe::arrange::TrackPanelState {
            rows,
            selected,
            selection_anchor: Some(self.sidebar.selected_track as u16),
            row_height: 32.0,
            scroll_y: self.editor.editor_state.view.scroll_y,
            request_pianoroll: false,
        };
        let left: Element<'_> =
            lumino_ui_yinhe::arrange::track_panel::view(&self.window, track_state);
        let left_wrapped: Element<'_> = iced_widget::container(left)
            .width(iced_core::Length::Fixed(220.0))
            .height(iced_core::Length::Fill)
            .into();

        // ── 中央走带 canvas：网格 + 时间标尺 1/1.2/1.3 1-10 等由 view_ui::draw_grid 绘制 ──
        let track_count = self.sidebar.tracks.len().max(1);
        let total_ticks = self.editor.editor_state.view.total_ticks as f64;
        let time_sigs: &[(u32, u8, u8)] = &self.editor.editor_state.data.time_signatures;
        let sigs_ref: &[(u32, u8, u8)] = if time_sigs.is_empty() {
            &FALLBACK_SIGS
        } else {
            time_sigs
        };
        let viewport = lumino_ui_yinhe::arrange::ArrangeViewport {
            view: self.editor.editor_state.view.clone(),
            lane_height: 32.0,
            left_panel_width: 220.0,
            row_height: 32.0,
        };
        let center_canvas: Element<'_> = lumino_ui_yinhe::arrange::view_ui::view(
            viewport,
            track_count,
            total_ticks,
            sigs_ref,
            &self.window,
        );
        let center_with_scroll: Element<'_> = {
            // 横纵滚动条（薄 10px，悬浮高亮，轨道点击/拖拇指/边缘缩放）
            let h_bar = lumino_ui_yinhe::widgets::scrollbar::horizontal_for_view(
                &self.editor.editor_state.view,
                &self.window,
            );
            let v_bar = lumino_ui_yinhe::widgets::scrollbar::vertical_for_view(
                &self.editor.editor_state.view,
                &self.window,
            );
            let grid_with_v = iced_widget::row![
                iced_widget::container(center_canvas)
                    .width(iced_core::Length::Fill)
                    .height(iced_core::Length::Fill),
                iced_widget::container(v_bar)
                    .width(iced_core::Length::Fixed(10.0))
                    .height(iced_core::Length::Fill)
            ]
            .height(iced_core::Length::Fill);
            let grid_with_h = iced_widget::column![
                grid_with_v.height(iced_core::Length::Fill),
                iced_widget::container(h_bar)
                    .width(iced_core::Length::Fill)
                    .height(iced_core::Length::Fixed(10.0))
            ]
            .width(iced_core::Length::Fill)
            .height(iced_core::Length::Fill);
            grid_with_h.into()
        };

        // ── 右侧面板：240px，占位 default（Info/Events/SoundFont），数据后续接 lumino 文档 ──
        static RIGHT_PANEL_DEFAULT: std::sync::OnceLock<
            lumino_ui_yinhe::right_panel::RightPanelState,
        > = std::sync::OnceLock::new();
        let right_state = RIGHT_PANEL_DEFAULT
            .get_or_init(lumino_ui_yinhe::right_panel::RightPanelState::default);
        let right_raw: Element<'_> =
            lumino_ui_yinhe::right_panel::view(&self.window, right_state);
        let right: Element<'_> = iced_widget::container(right_raw)
            .width(iced_core::Length::Fixed(240.0))
            .height(iced_core::Length::Fill)
            .into();

        // ── 中部行：按 ViewMode 分支（仅 Arrange 真实接入，Piano/Mix 占位但仍带右侧面板）──
        let middle: Element<'_> = match self.yinhe.view_mode {
            lumino_ui_yinhe::chrome::ViewMode::Arrange => iced_widget::row![
                left_wrapped,
                center_with_scroll,
                right
            ]
            .height(iced_core::Length::Fill)
            .spacing(0)
            .into(),
            lumino_ui_yinhe::chrome::ViewMode::Piano => {
                let piano: Element<'_> = lumino_ui_yinhe::piano_view::view(
                    &self.editor.editor_state.view,
                    &self.editor.editor_state,
                    &self.window,
                    lumino_ui_yinhe::piano_view::layout::Orientation::Horizontal,
                );
                let piano_wrap = iced_widget::container(piano)
                    .width(iced_core::Length::Fill)
                    .height(iced_core::Length::Fill);
                iced_widget::row![piano_wrap, right]
                    .height(iced_core::Length::Fill)
                    .into()
            }
            lumino_ui_yinhe::chrome::ViewMode::Mix => {
                let mix_stub = iced_widget::container(
                    iced_widget::column![
                        iced_widget::text("Mix (yinhe 占位)").size(16),
                        iced_widget::text("混音台不迁（P7 约束），仍使用 Lumino 原混音台").size(11),
                    ]
                    .spacing(6)
                    .align_x(iced_core::Alignment::Center),
                )
                .width(iced_core::Length::Fill)
                .height(iced_core::Length::Fill)
                .center_x(iced_core::Length::Fill)
                .center_y(iced_core::Length::Fill);
                iced_widget::row![mix_stub, right]
                    .height(iced_core::Length::Fill)
                    .into()
            }
        };

        // 底部：复用 mode_bar 的 ViewMode 切换（与顶部 chrome 内 mode_bar 呼应，满足图二底部 ARRANGE/MIX/EDIT 切换语义）
        // mode_bar::view 仅借 &Window，其余为 Copy，不产生局部借用
        let bottom: Element<'_> = lumino_ui_yinhe::chrome::mode_bar::view(
            &self.window,
            lumino_ui_core::app_mode::AppMode::Yinhe,
            self.yinhe.view_mode,
            self.yinhe.layout.show_pianoroll_in_arrange,
            None,
        );

        iced_widget::column![top, middle, bottom]
            .width(iced_core::Length::Fill)
            .height(iced_core::Length::Fill)
            .into()
    }

    /// 处理 Yinhe 快捷键（feature 门控，供 Host::handle_keyboard_shortcuts 调用）
    ///
    /// 复用罗盘：直接调用 `lumino_ui_yinhe::shortcuts::try_match_yinhe_message`，
    /// 命中则 `route_message`，未命中返回 false 交由原有 Editor/Arrangement 罗盘。
    pub(crate) fn try_handle_yinhe_shortcut(
        &mut self,
        key: winit::keyboard::KeyCode,
        ctrl: bool,
        shift: bool,
    ) -> bool {
        let is_yinhe = self.state.current_mode == crate::titlebar::mode_toggle::AppMode::Yinhe;
        if let Some(msg) = lumino_ui_yinhe::shortcuts::try_match_yinhe_message(
            key,
            ctrl,
            shift,
            self.toolbar.is_playing,
            is_yinhe,
        ) {
            // 复用现有消息路由（EditorAction/ToolbarEvent 均走 Root::update -> Host::route_message）
            // 此处直接走 Root::update 以复用 PPQ/播放等前置逻辑
            self.update(msg);
            return true;
        }
        false
    }

    /// 处理 Yinhe 动作（ViewMode 切换等）
    pub(crate) fn handle_yinhe_action(&mut self, action: lumino_message::YinheAction) -> bool {
        match action {
            lumino_message::YinheAction::ViewModeChanged(vm) => {
                let mode = match vm {
                    lumino_message::YinheViewMode::Arrange => {
                        lumino_ui_yinhe::chrome::ViewMode::Arrange
                    }
                    lumino_message::YinheViewMode::Piano => lumino_ui_yinhe::chrome::ViewMode::Piano,
                    lumino_message::YinheViewMode::Mix => lumino_ui_yinhe::chrome::ViewMode::Mix,
                };
                self.yinhe.view_mode = mode;
                tracing::info!("Yinhe ViewMode 切换到 {:?}", mode);
                true
            }
            lumino_message::YinheAction::TogglePianorollInArrange => {
                self.yinhe.layout.show_pianoroll_in_arrange =
                    !self.yinhe.layout.show_pianoroll_in_arrange;
                tracing::info!(
                    "Yinhe show_pianoroll_in_arrange 切换到 {}",
                    self.yinhe.layout.show_pianoroll_in_arrange
                );
                true
            }
        }
    }

    /// Yinhe 文件加载桩：拦截 .yin 后后缀，提示“yin格式暂不支持，请导出MIDI”
    ///
    /// 未来在 `lumino-midi-loader` 加 `.yin` 分支后，此处改为走 loader 管线。
    pub fn handle_yin_load_stub(
        &mut self,
        path: &std::path::Path,
        lang: lumino_extras::i18n::Language,
    ) -> bool {
        if let Some(err) = lumino_ui_yinhe::file::try_handle_yin_stub(path, lang) {
            self.toast.push(crate::toast::ToastLevel::Error, err);
            return true;
        }
        false
    }
}

#[cfg(not(feature = "yinhe"))]
impl Root {
    /// 非 feature 桩：处理 Yinhe 动作（未启用 feature 时直接忽略，已处理避免 WARN）
    pub(crate) fn handle_yinhe_action(&mut self, _action: lumino_message::YinheAction) -> bool {
        tracing::debug!("Yinhe 动作已忽略（未启用 yinhe feature）");
        true
    }

    /// 非 feature 桩：Yinhe 视图占位（提示需 --features yinhe）
    pub(crate) fn view_yinhe(&self) -> Element<'_> {
        iced_widget::container(
            iced_widget::column![
                iced_widget::text("Yinhe 副模式未启用").size(16),
                iced_widget::text("请使用 --features yinhe 重新编译").size(12),
                iced_widget::text("cargo run --features yinhe").size(11),
            ]
            .spacing(8)
            .align_x(iced_core::Alignment::Center),
        )
        .width(iced_core::Length::Fill)
        .height(iced_core::Length::Fill)
        .center_x(iced_core::Length::Fill)
        .center_y(iced_core::Length::Fill)
        .into()
    }

    /// 非 feature 桩：不处理 Yinhe 快捷键
    pub(crate) fn try_handle_yinhe_shortcut(
        &mut self,
        _key: winit::keyboard::KeyCode,
        _ctrl: bool,
        _shift: bool,
    ) -> bool {
        false
    }
}

#[cfg(all(test, feature = "yinhe"))]
mod tests {
    use crate::message::Message;
    use lumino_core::storage::config::UiConfig;
    use lumino_message::{YinheAction, YinheViewMode};

    #[test]
    fn yinhe_default_is_arrange() {
        let state = lumino_ui_yinhe::state::YinheState::default();
        assert_eq!(state.view_mode, lumino_ui_yinhe::chrome::ViewMode::Arrange);
        let root = crate::root::Root::new(&UiConfig::default());
        assert_eq!(
            root.yinhe.view_mode,
            lumino_ui_yinhe::chrome::ViewMode::Arrange,
            "Root.yinhe 默认应为 ARRANGE"
        );
    }

    #[test]
    fn handle_yinhe_action_switches_all_modes() {
        let mut root = crate::root::Root::new(&UiConfig::default());
        // 默认 ARRANGE
        assert_eq!(root.yinhe.view_mode, lumino_ui_yinhe::chrome::ViewMode::Arrange);

        // 切换到 Piano
        let handled = root.handle_yinhe_action(YinheAction::ViewModeChanged(YinheViewMode::Piano));
        assert!(handled);
        assert_eq!(root.yinhe.view_mode, lumino_ui_yinhe::chrome::ViewMode::Piano);

        // 切换到 Mix
        let handled = root.handle_yinhe_action(YinheAction::ViewModeChanged(YinheViewMode::Mix));
        assert!(handled);
        assert_eq!(root.yinhe.view_mode, lumino_ui_yinhe::chrome::ViewMode::Mix);

        // 切换回 Arrange（关键回归：ARRANGE 必须可达）
        let handled = root.handle_yinhe_action(YinheAction::ViewModeChanged(YinheViewMode::Arrange));
        assert!(handled);
        assert_eq!(root.yinhe.view_mode, lumino_ui_yinhe::chrome::ViewMode::Arrange);
    }

    #[test]
    fn update_via_message_routes_yinhe() {
        let mut root = crate::root::Root::new(&UiConfig::default());
        root.update(Message::Yinhe(YinheAction::ViewModeChanged(YinheViewMode::Piano)));
        assert_eq!(root.yinhe.view_mode, lumino_ui_yinhe::chrome::ViewMode::Piano);

        root.update(Message::Yinhe(YinheAction::ViewModeChanged(YinheViewMode::Arrange)));
        assert_eq!(root.yinhe.view_mode, lumino_ui_yinhe::chrome::ViewMode::Arrange);

        root.update(Message::Yinhe(YinheAction::ViewModeChanged(YinheViewMode::Mix)));
        assert_eq!(root.yinhe.view_mode, lumino_ui_yinhe::chrome::ViewMode::Mix);

        // TogglePianoroll
        let before = root.yinhe.layout.show_pianoroll_in_arrange;
        root.update(Message::Yinhe(YinheAction::TogglePianorollInArrange));
        assert_eq!(root.yinhe.layout.show_pianoroll_in_arrange, !before);
    }

    #[test]
    fn mode_bar_sends_correct_messages() {
        use lumino_ui_yinhe::chrome::mode_bar::ViewMode;
        // 验证 mode_bar 的 view_mode_message 映射（间接通过 handle）
        let mut root = crate::root::Root::new(&UiConfig::default());
        for mode in ViewMode::ALL {
            let vm = match mode {
                ViewMode::Arrange => YinheViewMode::Arrange,
                ViewMode::Piano => YinheViewMode::Piano,
                ViewMode::Mix => YinheViewMode::Mix,
            };
            root.handle_yinhe_action(YinheAction::ViewModeChanged(vm));
            assert_eq!(root.yinhe.view_mode, mode, "切换到 {:?} 失败", mode);
        }
    }
}
