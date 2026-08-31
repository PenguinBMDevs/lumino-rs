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

// ─── Root 扩展：Yinhe 状态访问（仅 feature 时） ─────────────────

#[cfg(feature = "yinhe")]
impl Root {
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
    ///   chrome::view(title+transport+mode_bar),
    ///   row![
    ///     arrange/piano/mix 中心区（按 YinheState.view_mode 分支）,
    ///     right_panel::view
    ///   ]
    /// ]
    /// ```
    /// 复用 lumino `Window.theme` 与 `AppMode::Yinhe` 高亮，i18n 按 lumino。
    ///
    /// P7 桩式实现：为通过 `'a` 生命周期检查（`Element<'a>` 借用 `&self`），
    /// 此处不直接调用 `chrome::view(&local_state)` / `right_panel::view(&local)`,
    /// 而是使用 `&self.window` 与 `self.yinhe` 的直接引用构造占位（无局部借用），
    /// 保证双形态 `cargo check` 通过；完整 `chrome/right_panel` 渲染在 P8 接入。
    pub(crate) fn view_yinhe(&self) -> Element<'_> {
        // 仅借用 &self.window（活得与 &self 一样久）与 &self.yinhe，不产生局部短生命周期借用
        let title = match self.yinhe.view_mode {
            lumino_ui_yinhe::chrome::ViewMode::Arrange => "Yinhe — ARRANGE",
            lumino_ui_yinhe::chrome::ViewMode::Piano => "Yinhe — PIANO",
            lumino_ui_yinhe::chrome::ViewMode::Mix => "Yinhe — MIX",
        };
        let desc = if self.yinhe.view_mode == lumino_ui_yinhe::chrome::ViewMode::Mix {
            "混音台不迁（P7 约束），仍使用 Lumino 原混音台"
        } else {
            "数据模型复用 Lumino 工程格式，yin 格式之后适配（初期不做）"
        };
        let bg = self.window.theme.palette().background;

        let header = iced_widget::container(
            iced_widget::row![
                iced_widget::text(title).size(14),
                iced_widget::Space::new().width(iced_core::Length::Fill),
                iced_widget::text(format!(
                    "split {:.2}  right {:.0}px",
                    self.yinhe.layout.arr_split, self.yinhe.layout.right_panel_width
                ))
                .size(11),
            ]
            .align_y(iced_core::Alignment::Center)
            .padding([6, 10]),
        )
        .width(iced_core::Length::Fill)
        .style(move |_t: &crate::Theme| iced_widget::container::Style {
            background: Some(iced_core::Background::Color(bg)),
            ..Default::default()
        });

        let center: Element<'_> = match self.yinhe.view_mode {
            lumino_ui_yinhe::chrome::ViewMode::Arrange => iced_widget::container(
                iced_widget::column![
                    iced_widget::text("Arrange (yinhe)").size(16),
                    iced_widget::text(desc).size(11),
                    iced_widget::text("走带视图桩（arrange::view_ui）").size(11),
                ]
                .spacing(6)
                .align_x(iced_core::Alignment::Center),
            )
            .width(iced_core::Length::Fill)
            .height(iced_core::Length::Fill)
            .center_x(iced_core::Length::Fill)
            .center_y(iced_core::Length::Fill)
            .into(),
            lumino_ui_yinhe::chrome::ViewMode::Piano => iced_widget::container(
                iced_widget::column![
                    iced_widget::text("Piano (yinhe)").size(16),
                    iced_widget::text(desc).size(11),
                    iced_widget::text("钢琴卷帘桩（piano_view）").size(11),
                ]
                .spacing(6)
                .align_x(iced_core::Alignment::Center),
            )
            .width(iced_core::Length::Fill)
            .height(iced_core::Length::Fill)
            .center_x(iced_core::Length::Fill)
            .center_y(iced_core::Length::Fill)
            .into(),
            lumino_ui_yinhe::chrome::ViewMode::Mix => iced_widget::container(
                iced_widget::column![
                    iced_widget::text("Mix (yinhe 占位)").size(16),
                    iced_widget::text(desc).size(11),
                ]
                .spacing(6)
                .align_x(iced_core::Alignment::Center),
            )
            .width(iced_core::Length::Fill)
            .height(iced_core::Length::Fill)
            .center_x(iced_core::Length::Fill)
            .center_y(iced_core::Length::Fill)
            .into(),
        };

        // 右侧面板桩：为避免局部 `RightPanelState` 借用导致 '局部引用逃逸，
        // 此处仅用占位文本，不调用 `right_panel::view(&local_state)`；
        // 完整 right_panel 渲染（info/event_browser/sf_list）在 P8 接入 lumino 数据后启用。
        let right_stub: Element<'_> = iced_widget::container(
            iced_widget::column![
                iced_widget::text("Right Panel (yinhe)").size(12),
                iced_widget::text("Info / Events / SoundFont").size(10),
            ]
            .spacing(4)
            .padding(8),
        )
        .width(iced_core::Length::Fixed(self.yinhe.layout.right_panel_width))
        .height(iced_core::Length::Fill)
        .style(|t: &crate::Theme| iced_widget::container::Style {
            background: Some(iced_core::Background::Color(
                t.extended_palette().background.weak.color,
            )),
            ..Default::default()
        })
        .into();

        let body = iced_widget::row![center, right_stub].height(iced_core::Length::Fill);

        iced_widget::column![header, body]
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
