//! 固定场景清单。
//!
//! 每个场景都显式锁定主题（Tokyo Night Storm）、语言（zh-CN）、图标标题栏
//! （`use_native_titlebar = false`）与逻辑窗口尺寸，避免受用户配置影响。

use lumino_core::storage::config::UiConfig;
use lumino_extras::i18n::Language;
use lumino_ui::Root;
use lumino_ui::state::root_state::DialogType;

/// 基线统一使用的 DPI 档位。
///
/// 逻辑窗口尺寸保持不变，物理输出尺寸按该倍率放大；这与 iced 的 HiDPI
/// 语义一致（布局用逻辑像素，光栅化用物理像素）。
pub const DPI_SCALES: [f32; 3] = [1.0, 1.25, 1.5];

/// 代表性基线场景。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneId {
    /// 主窗口整体外壳：标题栏、工具栏、左右侧栏、钢琴卷帘、状态栏。
    MainChrome,
    /// 设置对话框的「常规」页。
    SettingsGeneral,
    /// 设置对话框「常规」页（浅色主题）。
    SettingsGeneralLight,
    /// 设置对话框「常规」页（英文界面）。
    SettingsGeneralEn,
    /// 未保存更改确认对话框。
    SaveConfirmDialog,
    /// 空工程的钢琴卷帘骨架（键盘、标尺、播放指示线、滚动条、力度面板）。
    PianoRollSkeleton,
}

impl SceneId {
    /// 全部基线场景（顺序即 `capture_iced` 的输出顺序）。
    pub const ALL: [Self; 6] = [
        Self::MainChrome,
        Self::SettingsGeneral,
        Self::SettingsGeneralLight,
        Self::SettingsGeneralEn,
        Self::SaveConfirmDialog,
        Self::PianoRollSkeleton,
    ];

    /// 稳定的文件名词干（用于基线 PNG 命名）。
    pub fn slug(self) -> &'static str {
        match self {
            Self::MainChrome => "main_chrome",
            Self::SettingsGeneral => "settings_general",
            Self::SettingsGeneralLight => "settings_general_light",
            Self::SettingsGeneralEn => "settings_general_en",
            Self::SaveConfirmDialog => "save_confirm_dialog",
            Self::PianoRollSkeleton => "piano_roll_skeleton",
        }
    }

    /// 场景的**逻辑**窗口尺寸（不含 DPI 放大）。
    pub fn logical_size(self) -> (u32, u32) {
        match self {
            Self::MainChrome | Self::PianoRollSkeleton => (1600, 1000),
            Self::SettingsGeneral | Self::SettingsGeneralLight | Self::SettingsGeneralEn => {
                (1200, 800)
            }
            Self::SaveConfirmDialog => (560, 260),
        }
    }

    /// 基线 PNG 文件名，例如 `main_chrome-dpi125.png`。
    pub fn baseline_file_name(self, scale: f32) -> String {
        let dpi = (scale * 100.0).round() as u32;
        format!("{}-dpi{dpi}.png", self.slug())
    }
}

/// 一个已构造好的基线场景。
pub struct Scene {
    /// 场景标识。
    pub id: SceneId,
    /// 构造该场景时使用的固定配置。
    pub config: UiConfig,
    /// 待截图的 iced 根组件。
    pub root: Root,
}

impl Scene {
    /// 按固定配置构造场景。
    ///
    /// 在构造 `Root` 之前会先调用 [`lumino_ui_core::font::init`]，与生产启动
    /// 顺序一致（组件读取全局 `ui_font()`，渲染器默认字体也取同一结果）。
    pub fn build(id: SceneId) -> Self {
        let mut config = baseline_config();
        if id == SceneId::SettingsGeneralLight {
            // 浅色主题基线：用于验证 egui 调色板注入后的整窗渲染。
            config.theme = iced_core::Theme::Light.to_string();
        }
        if id == SceneId::SettingsGeneralEn {
            // 英文界面基线：用于验证设置对话框文案全部来自生产翻译表。
            config.language = Language::EnUs;
        }
        // 与生产启动顺序一致：先解析全局字体，再构造读取 `ui_font()` 的组件。
        let _font = lumino_ui_core::font::init(&config);

        let mut root = match id {
            SceneId::MainChrome => Root::new(&config),
            SceneId::PianoRollSkeleton => {
                let mut root = Root::new(&config);
                // 与 main_chrome 区分：缩小时间轴并滚到第 9 小节，展示标尺/
                // 骨架在非零时间位置的形态（空工程没有音符，网格与音符由宿主
                // 的 wgpu 渲染通道绘制，不在 iced 元素树内）。
                let view = &mut root.editor.editor_state.view;
                view.zoom_x = 0.05;
                let bar_pixels = 4.0 * f32::from(view.ppq) * view.zoom_x;
                view.scroll_x = 8.0 * bar_pixels;
                root
            }
            SceneId::SettingsGeneral
            | SceneId::SettingsGeneralLight
            | SceneId::SettingsGeneralEn => Root::new_settings_dialog(&config.theme, &config),
            SceneId::SaveConfirmDialog => {
                let mut root =
                    Root::new_dialog_with_config(&config.theme, DialogType::SaveConfirm, &config);
                // 生产路径由 Runner 先置 is_open 再开窗；直接构造时显式补齐，
                // 保证场景语义与真实对话框一致（当前视图本身不读取该字段）。
                root.state_mut().save_confirm_dialog.is_open = true;
                root
            }
        };

        // 首启帮助提示/菜单/动画若处于活动状态会污染基线：统一复位到静止态。
        let state = root.state_mut();
        state.help_tips.dismiss();
        state.is_menu_open = false;
        state.toggle_animation = Default::default();

        // 固定 FPS，避免状态栏出现随运行环境变化的计数（同时同步 Window.fps）。
        root.update(lumino_ui_core::Message::Window(
            lumino_ui_core::window::Event::FpsUpdate(60.0),
        ));

        Self { id, config, root }
    }
}

/// 基线统一配置：`UiConfig::default()` + Tokyo Night Storm + zh-CN。
///
/// 注意：`UiConfig::default().theme` 是 `"Light"`，因此这里必须显式覆盖主题；
/// 其余字段全部沿用默认值，以保证与“默认安装”行为一致。
pub fn baseline_config() -> UiConfig {
    UiConfig {
        theme: iced_core::Theme::TokyoNightStorm.to_string(),
        language: Language::ZhCn,
        use_native_titlebar: false,
        ..UiConfig::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_baseline_config_is_fixed() {
        let config = baseline_config();
        assert_eq!(config.theme, "Tokyo Night Storm");
        assert_eq!(config.language, Language::ZhCn);
        assert!(!config.use_native_titlebar);
    }

    #[test]
    fn test_scene_sizes() {
        assert_eq!(SceneId::MainChrome.logical_size(), (1600, 1000));
        assert_eq!(SceneId::SettingsGeneral.logical_size(), (1200, 800));
        assert_eq!(SceneId::SettingsGeneralLight.logical_size(), (1200, 800));
        assert_eq!(SceneId::SettingsGeneralEn.logical_size(), (1200, 800));
        assert_eq!(SceneId::SaveConfirmDialog.logical_size(), (560, 260));
        assert_eq!(SceneId::PianoRollSkeleton.logical_size(), (1600, 1000));
    }

    #[test]
    fn test_baseline_file_names() {
        assert_eq!(
            SceneId::MainChrome.baseline_file_name(1.0),
            "main_chrome-dpi100.png"
        );
        assert_eq!(
            SceneId::SettingsGeneral.baseline_file_name(1.25),
            "settings_general-dpi125.png"
        );
        assert_eq!(
            SceneId::SettingsGeneralLight.baseline_file_name(1.0),
            "settings_general_light-dpi100.png"
        );
        assert_eq!(
            SceneId::SettingsGeneralEn.baseline_file_name(1.25),
            "settings_general_en-dpi125.png"
        );
        assert_eq!(
            SceneId::SaveConfirmDialog.baseline_file_name(1.5),
            "save_confirm_dialog-dpi150.png"
        );
    }

    #[test]
    fn test_all_scenes_build_and_render_element_tree() {
        for id in SceneId::ALL {
            let scene = Scene::build(id);
            let expected = if id == SceneId::SettingsGeneralLight {
                iced_core::Theme::Light
            } else {
                iced_core::Theme::TokyoNightStorm
            };
            assert_eq!(scene.root.theme(), expected);
            let expected_language = if id == SceneId::SettingsGeneralEn {
                Language::EnUs
            } else {
                Language::ZhCn
            };
            assert_eq!(scene.config.language, expected_language);
            // view() 只做布局树构造，不触发 GPU；可安全在普通单测中执行。
            let element = scene.root.view();
            drop(element);
        }
    }

    #[test]
    fn test_save_confirm_scene_opens_dialog_state() {
        let mut scene = Scene::build(SceneId::SaveConfirmDialog);
        assert!(scene.root.state_mut().save_confirm_dialog.is_open);
    }

    #[test]
    fn test_settings_scene_starts_on_general_page() {
        let scene = Scene::build(SceneId::SettingsGeneral);
        assert_eq!(
            scene.root.settings().selected_menu_index,
            0,
            "设置面板索引 0 必须是「常规」页"
        );
    }
}
