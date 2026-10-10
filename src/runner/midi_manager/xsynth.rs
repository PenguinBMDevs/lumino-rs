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

        // REND-002：把当前文档期望的端口布局带入异步初始化（初始化期间可能尚未
        // 装载文档，装载后由 apply_midi_port_layout 再对齐）。
        let desired_midi_max_port = self.desired_midi_max_port;
        self.spawned_midi_max_port = desired_midi_max_port;

        // 在后台线程中初始化 XSynth
        let ui_config_clone = ui_config.clone();
        std::thread::spawn(move || {
            tracing::info!("XSynth: 后台线程开始初始化");

            let xsynth_result = Self::init_xsynth_blocking(&ui_config_clone, desired_midi_max_port);

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
    fn init_xsynth_blocking(ui_config: &UiConfig, midi_max_port: u8) -> MidiInitResult {
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
            // REND-002：文档端口布局（0 = 单端口/Midi）
            midi_max_port,
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

    /// 应用文档端口布局（REND-002 / DEBT-05 #122）。
    ///
    /// 同步路径（装载/关闭/导出/初始化对齐调用方）：先等待在途后台重建归还 API，
    /// 再按当前线程同步应用；失败保持旧布局并保留 desired。
    ///
    /// 端口编辑热路径请使用 [`Self::apply_midi_port_layout_deferred`]（后台防抖）。
    pub fn apply_midi_port_layout(&mut self, max_port: u8) {
        // 装载路径是 apply → create_additional_output：必须先收尾在途 worker，
        // 否则 API 被取走会导致播放输出创建失败（无声音）。
        self.drain_layout_apply();
        self.desired_midi_max_port = max_port;
        self.layout_desired.store(max_port, Ordering::SeqCst);
        match self.active_backend {
            SynthBackend::XSynth | SynthBackend::Lgs => {
                let Some(api) = self.api.as_mut() else {
                    return;
                };
                let started = std::time::Instant::now();
                match api.set_midi_port_layout(max_port) {
                    Ok(()) => {
                        self.spawned_midi_max_port = max_port;
                        tracing::info!(
                            "MIDI: 已应用端口布局 max_port={max_port}（耗时 {:?}）",
                            started.elapsed()
                        );
                    }
                    Err(e) => tracing::error!("MIDI: 应用端口布局失败（保持旧布局）: {e}"),
                }
            }
            _ => {
                tracing::info!(
                    "MIDI: 暂存端口布局 max_port={max_port}（等待软件合成后端就绪后应用）"
                );
            }
        }
    }
}
