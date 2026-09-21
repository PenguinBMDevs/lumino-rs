//! LGS (GPU) 后端异步初始化
//!
//! 从 `midi_manager.rs` 零逻辑变更拆分而来。

use super::*;

impl MidiManager {
    /// 启动 LGS (GPU) 异步初始化
    pub(super) fn start_lgs_async_init(&mut self, ui_config: &UiConfig) {
        if self.is_lgs_initializing {
            return;
        }

        if ui_config.soundfont_path.is_empty() {
            tracing::warn!("LGS (GPU) 异步初始化: 音色库路径未设置");
            return;
        }

        let path = PathBuf::from(&ui_config.soundfont_path);
        if !path.exists() {
            tracing::warn!("LGS (GPU) 异步初始化: 音色库文件不存在: {:?}", path);
            return;
        }

        tracing::info!("LGS (GPU): 启动后台初始化...");
        self.is_lgs_initializing = true;

        let (tx, rx) = channel();
        self.lgs_init_rx = Some(rx);

        let ui_config_clone = ui_config.clone();
        std::thread::spawn(move || {
            tracing::info!("LGS (GPU): 后台线程开始初始化");

            let lgs_result = Self::init_lgs_blocking(&ui_config_clone);

            match &lgs_result {
                Ok(_) => tracing::info!("LGS (GPU): 后台初始化成功"),
                Err(e) => tracing::warn!("LGS (GPU): 后台初始化失败: {}", e),
            }

            let init_result = match lgs_result {
                Ok((api, output)) => LgsInitResult::Success { api, output },
                Err(e) => LgsInitResult::Failed(e),
            };

            let _ = tx.send(init_result);
        });
    }

    /// 阻塞式初始化 LGS (GPU)（用于后台线程）
    fn init_lgs_blocking(ui_config: &UiConfig) -> MidiInitResult {
        use lumino_midi_io::ApiKind;

        let path = PathBuf::from(&ui_config.soundfont_path);
        let api_kind = ApiKind::Lgs {
            soundfont_path: path,
            sample_rate: ui_config.lgs_sample_rate,
            block_size: ui_config.lgs_block_size,
            max_voices_per_key: ui_config.lgs_max_voices_per_key,
            use_sinc: ui_config.lgs_use_sinc,
            velocity_filter_threshold: ui_config.lgs_velocity_filter_threshold,
            audio_output_device: ui_config.audio_output_device.clone(),
        };

        let api = lumino_midi_io::new_api(&api_kind)
            .map_err(|e| format!("初始化 LGS (GPU) MIDI API 失败: {:?}", e))?;

        if let Some(version) = api.version() {
            tracing::info!("LGS (GPU): 音频后端已初始化 (version: {})", version);
        }

        let outputs = api
            .outputs()
            .map_err(|e| format!("获取 LGS (GPU) 输出设备失败: {:?}", e))?;

        let output = outputs
            .first()
            .ok_or("未找到可用的 LGS (GPU) MIDI 输出设备")?;

        let conn = api
            .open_output(output.id)
            .map_err(|e| format!("打开 LGS (GPU) 输出连接失败: {:?}", e))?;

        Ok((api, conn))
    }
}
