//! 固定视觉对比场景。

use std::path::PathBuf;

use crate::chrome::{self, ChromeState};
use crate::dialog::{self, DialogState};
use crate::settings::{self, SettingsState};

/// 与 旧版 基线一致的 DPI 档位。
pub const DPI_SCALES: [f32; 3] = [1.0, 1.25, 1.5];

/// 代表性场景。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneId {
    /// 主窗口外壳。
    MainChrome,
    /// 设置对话框「常规」页。
    SettingsGeneral,
    /// 未保存更改确认对话框。
    SaveConfirmDialog,
    /// 钢琴卷帘骨架（第 9 小节起）。
    PianoRollSkeleton,
}

impl SceneId {
    /// 全部场景。
    pub const ALL: [Self; 4] = [
        Self::MainChrome,
        Self::SettingsGeneral,
        Self::SaveConfirmDialog,
        Self::PianoRollSkeleton,
    ];

    /// 文件名干。
    pub fn slug(self) -> &'static str {
        match self {
            Self::MainChrome => "main_chrome",
            Self::SettingsGeneral => "settings_general",
            Self::SaveConfirmDialog => "save_confirm_dialog",
            Self::PianoRollSkeleton => "piano_roll_skeleton",
        }
    }

    /// 逻辑尺寸（与 旧版 基线一致）。
    pub fn logical_size(self) -> (u32, u32) {
        match self {
            Self::MainChrome | Self::PianoRollSkeleton => (1600, 1000),
            Self::SettingsGeneral => (1200, 800),
            Self::SaveConfirmDialog => (560, 260),
        }
    }
}

/// 场景运行状态（跨帧保留图标缓存与交互状态）。
pub struct SceneState {
    /// 当前场景。
    pub scene: SceneId,
    /// 主窗口状态。
    pub chrome: ChromeState,
    /// 设置状态。
    pub settings: SettingsState,
    /// 对话框状态。
    pub dialog: DialogState,
}

impl SceneState {
    /// 构造场景状态。
    pub fn new(scene: SceneId) -> Self {
        let chrome = if scene == SceneId::PianoRollSkeleton {
            ChromeState::piano_roll_skeleton()
        } else {
            ChromeState::default()
        };
        Self {
            scene,
            chrome,
            settings: SettingsState::default(),
            dialog: DialogState::default(),
        }
    }

    /// 渲染当前场景。
    pub fn show(&mut self, ui: &mut egui::Ui) {
        // `egui_kittest` 通过 eframe 的默认 `CentralPanel` 提供一个内缩 8px 的 Ui；
        // 生产接线的根 Ui 也是同样语义。这里重建一个铺满视口的根 Ui，
        // 让 Panel 的外框与窗口边缘对齐（否则四周会留下 8px 黑边）。
        let full = ui.ctx().viewport_rect();
        let mut root = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(full)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        root.set_clip_rect(full);

        match self.scene {
            SceneId::MainChrome | SceneId::PianoRollSkeleton => {
                chrome::show(&mut root, &mut self.chrome);
            }
            SceneId::SettingsGeneral => settings::show(&mut root, &mut self.settings),
            SceneId::SaveConfirmDialog => dialog::show(&mut root, &mut self.dialog),
        }
    }
}

/// 默认 egui 基线输出目录。
pub fn baseline_dir() -> PathBuf {
    workspace_root()
        .join("tests")
        .join("ui-baseline")
        .join("egui")
}

/// 基线文件名。
pub fn baseline_file_name(scene: SceneId, scale: f32) -> String {
    let dpi = (scale * 100.0).round() as u32;
    format!("{}-dpi{dpi}.png", scene.slug())
}

fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(3)
        .map(PathBuf::from)
        .unwrap_or(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_sizes_match_legacy_baseline() {
        assert_eq!(SceneId::MainChrome.logical_size(), (1600, 1000));
        assert_eq!(SceneId::SettingsGeneral.logical_size(), (1200, 800));
        assert_eq!(SceneId::SaveConfirmDialog.logical_size(), (560, 260));
        assert_eq!(SceneId::PianoRollSkeleton.logical_size(), (1600, 1000));
    }

    #[test]
    fn baseline_names_are_stable() {
        assert_eq!(
            baseline_file_name(SceneId::MainChrome, 1.25),
            "main_chrome-dpi125.png"
        );
    }
}
