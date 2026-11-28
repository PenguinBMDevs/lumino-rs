//! 音频导出窗口事件处理

use std::sync::Arc;

use crate::runner::RunnerInner;
use lumino_message::events::window::dialog::AudioExportConfig;

mod channels;
mod config;
mod guards;
mod render;

impl RunnerInner {
    // clippy::boxed_local: Box 来自 `dialog::Event::StartAudioExport` 枚举解包，
    // 用于 UI→Runner 消息零拷贝传递（见 Message 枚举体积守卫测试）。
    #[allow(clippy::boxed_local)]
    pub(crate) fn handle_start_audio_export(
        &mut self,
        config: Box<AudioExportConfig>,
        document: Option<Arc<lumino_midi_loader::MidiDocument>>,
    ) {
        // DEBT-07 #124：导出可重入守卫——已有音频导出（控制句柄存在）或视频导出
        // 进行中时拒绝，避免进度通道/控制句柄被覆盖、两线程并发写盘。
        if self.reject_if_export_busy() {
            return;
        }

        let config = *config;

        // 根据是否有内存中的 MidiDocument 选择渲染模式
        let mode_str = if document.is_some() {
            "内存模式（零拷贝）"
        } else {
            "文件模式"
        };

        // REND-019 PR-D（§1.5 L4）：TDR 窗口期拒绝启动 GPU 音频导出——编辑器共享
        // device 丢失/重连进行中时，导出新建的 GPU 设备大概率创建失败或立即丢失；
        // 给出明确提示，避免导出中途以晦涩错误收场。
        if self.reject_gpu_export_if_device_lost(config.backend) {
            return;
        }

        tracing::info!(
            "开始音频导出 [{mode_str}]: MIDI={}, SF2={}",
            config.midi_path,
            config.soundfont_path
        );

        // 1. 创建进度通道，将渲染进度发回主线程更新 UI
        // 1.1 创建导出控制句柄（暂停/中止）
        let (progress_tx, control, progress_cb) = self.prepare_export_channels();

        // REND-005：端口音色库映射来自工程态（与实时/保存共用同一权威字段）。
        let port_soundfonts = self.window_state.window.ui().get_project_port_soundfonts();

        let render_config = config::build_render_config(
            config,
            self.window_state
                .storage
                .config
                .get()
                .ui
                .lgs_offline_block_size,
            progress_cb,
            &control,
            port_soundfonts,
        );

        // 2. tempo 无需覆盖（2026-10-20 审计 §三）：`EditorData` 的 tempo 写入入口
        //    （set_tempo_points / set_tempo_point / add / remove）都会经
        //    `sync_tempo_to_document` 写回 `document.tempo_changes`，故文档快照里
        //    已是编辑后的值。原 `editor_tempos` 覆盖逻辑已删除。

        // 3. 在后台线程执行音频渲染，避免阻塞主线程 UI
        render::spawn_render_task(render_config, document, progress_tx);
    }
}
