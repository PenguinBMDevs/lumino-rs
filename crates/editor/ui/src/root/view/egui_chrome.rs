//! 生产状态 → egui 外壳视图模型。
//!
//! 只做数据搬运，不含任何渲染逻辑；渲染在 `lumino_ui_egui::chrome::show_bands`。
//! 这样叠加层与离线场景共用同一份绘制代码，也避免 `lumino-ui-egui` 反向依赖 上一代。
//!
//! 结构：`chrome_model`（主窗口 / 对话框分流）、`sidebar_model`（左侧栏）、
//! `piano_roll_model`（卷帘骨架）；本文件保留首屏模型与共享的色彩/分组转换。

mod apply_actions;
mod area_palette;
mod arrangement;
mod audio_export_panel;
mod bend_buttons;
mod chrome_model;
mod context_menu;
mod curve_preview;
mod dialogs;
mod editor_buttons;
mod editor_overlay;
mod egg;
mod first_screen;
mod image_to_midi;
mod marquee;
mod mask_panel;
mod material_delete;
mod menu;
mod mixer;
mod palette;
mod piano_roll_model;
mod remote_cursors;
mod renderer_panel;
mod right_sidebar;
mod settings_audio;
mod settings_pages;
mod settings_ui;
mod shape_canvas;
mod shape_drawer;
mod shape_edit;
mod shape_list;
mod shape_preview;
mod sidebar_model;
mod sidebar_panel;
mod tempo_max_bpm;
mod text_tool;
mod titlebar;
mod toast;
mod tool_panel;
mod toolbar;
mod velocity_panel;
mod video_clip_timeline;
mod video_export;
mod video_preview;

use lumino_core::i18n::main_translations;
use lumino_ui_egui::chrome::piano_roll::PianoRollView;
use lumino_ui_egui::chrome::{ChromeModel, SidebarGroup};

use crate::root::Root;
use crate::sidebar::Sidebar;

/// 上一代 分组 → egui 分组。
fn sidebar_group(group: crate::sidebar::GroupId) -> SidebarGroup {
    match group {
        crate::sidebar::GroupId::PianoRoll => SidebarGroup::PianoRoll,
        crate::sidebar::GroupId::Project => SidebarGroup::Project,
        crate::sidebar::GroupId::Waterfall => SidebarGroup::Waterfall,
        crate::sidebar::GroupId::Renderer => SidebarGroup::Renderer,
    }
}

/// 线性浮点颜色 → egui 颜色（sRGB 直通）。
pub(super) fn color32(color: lumino_ui_core::geom::Color) -> egui::Color32 {
    let to_u8 = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    egui::Color32::from_rgb(to_u8(color.r), to_u8(color.g), to_u8(color.b))
}

impl Sidebar {
    /// 左侧栏本帧是否需要让位给别处绘制（当前恒为 `false`）。
    ///
    /// 历史上这里用于「重命名输入框 / 拖拽插入线 / 颜色·端口·通道选择器 / 宽度拖拽
    /// 仍由上一代 UI 层绘制，叠加层必须让位」的情形；这些交互后来全部迁到 egui
    ///（`chrome/sidebar/menu.rs`、`track_list.rs`），判定位保留，避免将来再引入
    /// 同类状态时把判断散落回各处。
    fn has_transient_ui(&self) -> bool {
        // 已无需要让位的情形：拖拽排序、右键菜单、行内重命名、取色 / 端口 / 通道
        // 选择器、宽度拖拽全部由 egui 绘制并接管。常量 `false` 让上方分支可被优化掉。
        false
    }
}

impl Root {
    /// 本帧「无面板」第一屏模型（`None` = 不显示）。
    ///
    /// `project_open` 由宿主传入（`Host::onion_backend_locked()` 背后的
    /// `MidiLoadLock`：真实 MIDI/工程载入为真，启动 / 新建 / 关闭工程为假）：
    /// - 无工程 → 左右分栏，右栏 5 个操作按钮（新建 / 打开文件 / 打开文件夹 /
    ///   设置 / 连接协作房间）；
    /// - 有工程 → 只有品牌块（居中），与旧行为一致。
    pub(crate) fn first_screen_model(
        &self,
        project_open: bool,
    ) -> Option<lumino_ui_egui::chrome::first_screen::FirstScreenModel> {
        use lumino_ui_egui::chrome::first_screen::{
            FirstScreenAction, FirstScreenButton, FirstScreenModel,
        };

        if !self.shows_first_screen() {
            return None;
        }
        let opacity = self.settings.display.first_screen_opacity.clamp(0.0, 1.0);
        if project_open {
            return Some(FirstScreenModel::brand_only(opacity));
        }
        let t = main_translations(self.settings.display.language);
        let button = |action: FirstScreenAction, label: &str| FirstScreenButton {
            action,
            label: label.to_owned(),
        };
        Some(FirstScreenModel {
            opacity,
            actions: vec![
                button(FirstScreenAction::NewProject, t.first_screen_new_project),
                button(FirstScreenAction::OpenProjectFile, t.first_screen_open_file),
                button(
                    FirstScreenAction::OpenProjectFolder,
                    t.first_screen_open_folder,
                ),
                button(FirstScreenAction::OpenSettings, t.first_screen_settings),
                button(
                    FirstScreenAction::OpenCollaboration,
                    t.first_screen_collaboration,
                ),
            ],
        })
    }

    /// 本帧的卷帘骨架模型（生产渲染线程据此决定是否让出标尺）。
    ///
    /// 键盘列与滚动条**只由 egui 绘制**（gfx 网格着色器 discard 键盘列），
    /// 因此只要处于卷帘视图，模型就必须存在；模型为空只允许发生在
    /// 非卷帘视图（走带 / 导出面板 / 卷帘关闭）或画布尚未布局的首帧。
    pub(crate) fn egui_piano_roll_view(&self) -> Option<PianoRollView> {
        self.egui_piano_roll_model()
    }

    /// 构造 egui 外壳快照（标题栏 + 工具栏 + 左侧栏 + 状态栏，只读）。
    ///
    /// 测试使用固定视口宽度；真实叠加层走 [`Self::egui_chrome_model_for`]，
    /// 因为工具栏折叠（溢出菜单）依赖实际可用宽度。
    #[cfg(test)]
    pub(crate) fn egui_chrome_model(&self) -> ChromeModel {
        self.egui_chrome_model_for(1280.0)
    }

    /// 原生运行时（`lumino-ui-runtime`）使用的整窗外壳模型。
    ///
    /// 与叠加层共用同一份 `egui_chrome_model_for`；对外公开是为了让
    /// winit + wgpu + egui 直连运行时在不依赖 `Host` 的情况下渲染生产外壳。
    pub fn egui_chrome_model_native(&self, viewport_width: f32) -> ChromeModel {
        self.egui_chrome_model_for(viewport_width)
    }
}

impl Root {
    /// 状态栏动作 → 生产消息。
    pub(crate) fn statusbar_action_message(
        &self,
        action: lumino_ui_egui::chrome::statusbar::StatusbarAction,
    ) -> crate::Message {
        use lumino_ui_egui::chrome::statusbar::StatusbarAction;
        match action {
            StatusbarAction::OpenMemoryMonitor => {
                crate::toolbar::Event::open_memory_monitor_dialog()
            }
        }
    }
}

#[cfg(test)]
mod tests;
