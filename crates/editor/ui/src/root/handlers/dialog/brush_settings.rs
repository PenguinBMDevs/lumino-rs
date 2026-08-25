//! 画刷「绘制行为」对话框处理器
//!
//! - 主窗侧 `OpenDialog(config)`：触发 Runner 打开独立 OS 窗口（注入当前配置）。
//! - 对话框侧：编辑本地草稿 `root.state.brush_settings_draft`，
//!   `Save` 通过 `DialogResult::BrushSettings` 回传主窗应用。

use lumino_core::BrushConfig;
use lumino_message::BrushSettingsAction;

use crate::host::DialogResult;
use crate::root::Root;

use super::DialogHandler;

impl DialogHandler {
    pub(super) fn handle_brush_settings(
        &self,
        root: &mut Root,
        action: BrushSettingsAction,
    ) -> Option<crate::message::Message> {
        match action {
            BrushSettingsAction::OpenDialog(config) => {
                // 主窗侧：请求打开独立 OS 对话框，携带当前画刷配置作为初始草稿。
                tracing::info!("Root: 请求打开画刷绘制行为对话框");
                crate::event::emit(crate::event::Event::Window(
                    crate::event::window::Event::open_brush_settings_dialog(config),
                ));
            }
            BrushSettingsAction::CloseDialog => {
                root.state.dialog_result = Some(DialogResult::Cancel);
            }
            BrushSettingsAction::Cancel => {
                root.state.dialog_result = Some(DialogResult::Cancel);
            }
            BrushSettingsAction::Save => {
                let config = root.state.brush_settings_draft.clone();
                tracing::info!("画刷绘制行为: 保存配置（粗细度={}）", config.thickness);
                root.state.dialog_result = Some(DialogResult::BrushSettings(config));
            }
            BrushSettingsAction::ThicknessChanged(t) => {
                let t = t.clamp(BrushConfig::MIN_THICKNESS, BrushConfig::MAX_THICKNESS);
                root.state.brush_settings_draft.set_thickness(t);
            }
            BrushSettingsAction::LevelTrackChanged(level, track) => {
                root.state.brush_settings_draft.set_track(level, track);
            }
            BrushSettingsAction::AddLevel(after) => {
                let draft = &mut root.state.brush_settings_draft;
                if draft.thickness < BrushConfig::MAX_THICKNESS {
                    let idx = (after + 1).min(draft.tracks.len());
                    draft.tracks.insert(idx, None);
                    draft.thickness = draft.tracks.len() as u8;
                }
            }
            BrushSettingsAction::RemoveLevel(level) => {
                let draft = &mut root.state.brush_settings_draft;
                if draft.thickness > BrushConfig::MIN_THICKNESS && level < draft.tracks.len() {
                    draft.tracks.remove(level);
                    draft.thickness = draft.tracks.len() as u8;
                }
            }
        }
        None
    }
}
