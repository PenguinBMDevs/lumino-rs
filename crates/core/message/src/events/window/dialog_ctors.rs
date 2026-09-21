use std::sync::Arc;

use super::*;

impl Event {
    // ── 对话框构造函数 ──

    /// 构造对话框事件（Box 化以减小 `Event` / `Message` 枚举体积）。
    ///
    /// 注意：`Box::new` 在稳定版 Rust 中暂非 const，故本函数非 `const fn`；
    /// 依赖它的对话框构造函数同样为非 const（无实际 const 用途）。
    pub fn dialog(ev: dialog::Event) -> Self {
        Self::Dialog(Box::new(ev))
    }

    /// 构造打开自定义精度对话框事件
    pub fn open_custom_precision_dialog() -> Self {
        Self::dialog(dialog::Event::OpenCustomPrecisionDialog)
    }
    /// 构造打开画刷「绘制行为」对话框事件（携带当前画刷配置）
    pub fn open_brush_settings_dialog(config: lumino_core::BrushConfig) -> Self {
        Self::dialog(dialog::Event::OpenBrushSettingsDialog(config))
    }
    /// 构造关闭自定义精度对话框事件
    pub fn close_custom_precision_dialog() -> Self {
        Self::dialog(dialog::Event::CloseCustomPrecisionDialog)
    }
    /// 构造应用自定义精度事件
    pub fn apply_custom_precision(numerator: u32, denominator: u32) -> Self {
        Self::dialog(dialog::Event::ApplyCustomPrecision(numerator, denominator))
    }
    /// 构造打开加载确认对话框事件
    pub fn open_load_confirm_dialog(path: String, size_mb: f64) -> Self {
        Self::dialog(dialog::Event::OpenLoadConfirmDialog { path, size_mb })
    }
    /// 构造打开协作对话框事件
    pub fn open_collaboration_dialog() -> Self {
        Self::dialog(dialog::Event::OpenCollaborationDialog)
    }
    /// 构造关闭协作对话框事件
    pub fn close_collaboration_dialog() -> Self {
        Self::dialog(dialog::Event::CloseCollaborationDialog)
    }
    /// 构造打开变速对话框事件
    pub fn open_speed_change_dialog() -> Self {
        Self::dialog(dialog::Event::OpenSpeedChangeDialog)
    }
    /// 构造关闭变速对话框事件
    pub fn close_speed_change_dialog() -> Self {
        Self::dialog(dialog::Event::CloseSpeedChangeDialog)
    }
    /// 构造确认变速事件
    pub fn confirm_speed_change(factor: f32) -> Self {
        Self::dialog(dialog::Event::ConfirmSpeedChange(factor))
    }
    /// 构造打开批量编辑对话框事件
    pub fn open_batch_edit_dialog() -> Self {
        Self::dialog(dialog::Event::OpenBatchEditDialog)
    }
    /// 构造关闭批量编辑对话框事件
    pub fn close_batch_edit_dialog() -> Self {
        Self::dialog(dialog::Event::CloseBatchEditDialog)
    }
    /// 构造确认批量编辑事件
    pub fn confirm_batch_edit(velocity: String, gate: String, key: String, tick: String) -> Self {
        Self::dialog(dialog::Event::ConfirmBatchEdit {
            velocity,
            gate,
            key,
            tick,
        })
    }
    /// 构造打开视频导出对话框事件
    pub fn open_video_export_dialog() -> Self {
        Self::dialog(dialog::Event::OpenVideoExportDialog)
    }
    /// 构造关闭视频导出对话框事件
    pub fn close_video_export_dialog() -> Self {
        Self::dialog(dialog::Event::CloseVideoExportDialog)
    }
    /// 构造打开工程设置对话框事件
    pub fn open_project_settings_dialog() -> Self {
        Self::dialog(dialog::Event::OpenProjectSettingsDialog)
    }
    /// 构造关闭工程设置对话框事件
    pub fn close_project_settings_dialog() -> Self {
        Self::dialog(dialog::Event::CloseProjectSettingsDialog)
    }
    /// 构造打开内存监控对话框事件
    pub fn open_memory_monitor_dialog() -> Self {
        Self::dialog(dialog::Event::OpenMemoryMonitorDialog)
    }
    /// 构造关闭内存监控对话框事件
    pub fn close_memory_monitor_dialog() -> Self {
        Self::dialog(dialog::Event::CloseMemoryMonitorDialog)
    }
    /// 构造打开找回音轨对话框事件
    pub fn open_recover_track_dialog() -> Self {
        Self::dialog(dialog::Event::OpenRecoverTrackDialog)
    }
    /// 构造关闭找回音轨对话框事件
    pub fn close_recover_track_dialog() -> Self {
        Self::dialog(dialog::Event::CloseRecoverTrackDialog)
    }
    /// 构造删除音轨事件
    pub fn delete_track(payload: track::TrackDeletionPayload) -> Self {
        Self::Track(track::Event::DeleteTrack(payload))
    }
    /// 构造恢复音轨事件
    pub fn restore_track(path: std::path::PathBuf, original_index: usize) -> Self {
        Self::Track(track::Event::RestoreTrack {
            path,
            original_index,
        })
    }
    /// 构造永久删除音轨事件
    pub fn permanently_delete_track(path: std::path::PathBuf, track_id: u16) -> Self {
        Self::Track(track::Event::PermanentlyDeleteTrack { path, track_id })
    }
    /// 构造找回音轨对话框扫描完成事件
    pub fn recover_track_dialog_scanned(entries: Vec<track::RecoverTrackEntryPayload>) -> Self {
        Self::Track(track::Event::RecoverTrackDialogScanned(entries))
    }
    /// 构造音轨已恢复事件
    pub fn track_restored(payload: track::TrackDeletionPayload) -> Self {
        Self::Track(track::Event::TrackRestored(payload))
    }
    /// 构造音轨已永久删除事件
    pub fn track_permanently_deleted(track_id: u16) -> Self {
        Self::Track(track::Event::TrackPermanentlyDeleted { track_id })
    }
    /// 构造应用工程设置事件
    pub fn apply_project_settings(
        title: String,
        tempo: f64,
        copyright: String,
        author: String,
        time_signatures: Vec<(u32, u8, u8)>,
    ) -> Self {
        Self::dialog(dialog::Event::ApplyProjectSettings {
            title,
            tempo,
            copyright,
            author,
            time_signatures,
        })
    }
    /// 构造开始音频导出事件
    pub fn start_audio_export(
        config: dialog::AudioExportConfig,
        document: Option<Arc<lumino_midi_loader::MidiDocument>>,
    ) -> Self {
        Self::dialog(dialog::Event::StartAudioExport {
            config: Box::new(config),
            document,
        })
    }
    /// 构造开始视频导出事件
    pub fn start_video_export(
        config: dialog::VideoExportConfig,
        document: Option<Arc<lumino_midi_loader::MidiDocument>>,
    ) -> Self {
        Self::dialog(dialog::Event::StartVideoExport {
            config: Box::new(config),
            document,
        })
    }
}
