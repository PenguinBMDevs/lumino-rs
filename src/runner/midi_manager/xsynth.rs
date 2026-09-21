//! XSynth 后端异步初始化
//!
//! 从 `midi_manager.rs` 零逻辑变更拆分而来。

use super::*;

impl MidiManager {
    /// 启动 XSynth 异步初始化
    pub(super) fn start_xsynth_async_init(&mut self, ui_config: &UiConfig) {
        if self.is_xsynth_initializing {
            return;
        }

        if ui_config.soundfont_path.is_empty() {
            tracing::warn!("XSynth 异步初始化: 音色库路径未设置");
            return;
        }

        let path = PathBuf::from(&ui_config.soundfont_path);
        if !path.exists() {
            tracing::warn!("XSynth 异步初始化: 音色库文件不存在: {:?}", path);
            return;
        }

        tracing::info!("XSynth: 启动后台初始化...");
        self.is_xsynth_initializing = true;

        let (tx, rx) = channel();
        self.xsynth_init_rx = Some(rx);

        // 在后台线程中初始化 XSynth
        let ui_config_clone = ui_config.clone();
        std::thread::spawn(move || {
            tracing::info!("XSynth: 后台线程开始初始化");

            let xsynth_result = Self::init_xsynth_blocking(&ui_config_clone);

            match &xsynth_result {
                Ok(_) => tracing::info!("XSynth: 后台初始化成功"),
                Err(e) => tracing::warn!("XSynth: 后台初始化失败: {}", e),
            }

            let init_result = match xsynth_result {
                Ok((api, output)) => XSynthInitResult::Success { api, output },
                Err(e) => XSynthInitResult::Failed(e),
            };

            let _ = tx.send(init_result);
        });
    }

    /// 阻塞式初始化 XSynth（用于后台线程）
    fn init_xsynth_blocking(ui_config: &UiConfig) -> MidiInitResult {
        use lumino_midi_io::{ApiKind, api::xsynth::XSynthOptions};

        let path = PathBuf::from(&ui_config.soundfont_path);
        let api_kind = ApiKind::XSynth {
            soundfont_path: path,
        };

        let options = XSynthOptions {
            buffer_ms: ui_config.xsynth_buffer_ms,
            max_voices_per_key: ui_config.xsynth_max_voices_per_key,
            sample_rate: ui_config.xsynth_sample_rate,
            // 每通道上限关闭，改用跨通道全局上限：未配置时给自动（引擎默认 10000），
            // 由负载治理器按软目标比例决定实际运行目标。
            max_voices_per_channel: None,
            global_max_voices: Some(ui_config.xsynth_global_voice_limit.unwrap_or(10_000).max(1)),
            voice_target_ratio: ui_config.xsynth_voice_target_ratio,
            soft_nps_gate: ui_config.xsynth_soft_nps_gate,
            audio_output_device: ui_config.audio_output_device.clone(),
        };

        let api = lumino_midi_io::new_api_with_options(&api_kind, Some(options))
            .map_err(|e| format!("初始化 MIDI API 失败: {:?}", e))?;

        // 诊断：打印音频后端信息
        if let Some(version) = api.version() {
            tracing::info!("XSynth: 音频后端已初始化 (version: {})", version);
        }
        tracing::info!(
            "XSynth: 采样率={}Hz, buffer={}ms, 线程=按机器强制, 每键同音数={}, 全局声部上限={}, 软目标比例={:.3}, 保险闸={}",
            ui_config.xsynth_sample_rate,
            ui_config.xsynth_buffer_ms,
            ui_config
                .xsynth_max_voices_per_key
                .map_or_else(|| "不限".to_string(), |v| v.to_string()),
            ui_config
                .xsynth_global_voice_limit
                .map_or_else(|| "自动(10000)".to_string(), |v| v.to_string()),
            ui_config.xsynth_voice_target_ratio,
            if ui_config.xsynth_soft_nps_gate {
                "开"
            } else {
                "关"
            },
        );
        tracing::info!(
            "XSynth: 如需强制使用 ALSA 而非 JACK，设置环境变量 XSYNTH_AUDIO_BACKEND=alsa"
        );

        let outputs = api
            .outputs()
            .map_err(|e| format!("获取输出设备失败: {:?}", e))?;

        let output = outputs.first().ok_or("未找到可用的 MIDI 输出设备")?;

        let conn = api
            .open_output(output.id)
            .map_err(|e| format!("打开输出连接失败: {:?}", e))?;

        Ok((api, conn))
    }
}
