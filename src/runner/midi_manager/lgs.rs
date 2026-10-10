//! LGS (GPU) 后端异步初始化
//!
//! 从 `midi_manager.rs` 零逻辑变更拆分而来。

use super::*;
use lumino_core::storage::config::LGS_AUTO_MAX_VOICES;

impl MidiManager {
    /// 启动 LGS (GPU) 异步初始化
    pub(super) fn start_lgs_async_init(&mut self, ui_config: &UiConfig) {
        if self.is_lgs_initializing {
            return;
        }

        if ui_config.soundfont_path.is_empty() {
            tracing::warn!("LGS (GPU) 异步初始化: 音色库路径未设置");
            // #127 问题1：不得静默降级——生成可见提示（Runner 每帧写入状态栏）。
            self.pending_backend_notice = Some(
                "LGS 未生效：未设置音色库，当前使用 System 后端；请在设置中加载音色库（.sf2/.sfz）"
                    .to_string(),
            );
            return;
        }

        let path = PathBuf::from(&ui_config.soundfont_path);
        if !path.exists() {
            tracing::warn!("LGS (GPU) 异步初始化: 音色库文件不存在: {:?}", path);
            self.pending_backend_notice = Some(format!(
                "LGS 未生效：音色库文件不存在（{}），当前使用 System 后端",
                path.display()
            ));
            return;
        }

        tracing::info!("LGS (GPU): 启动后台初始化...");
        self.is_lgs_initializing = true;

        let (tx, rx) = channel();
        self.lgs_init_rx = Some(rx);

        // REND-002：把当前文档期望的端口布局带入异步初始化（初始化期间可能尚未
        // 装载文档，装载后由 apply_midi_port_layout 再对齐）。
        let desired_midi_max_port = self.desired_midi_max_port;
        self.spawned_midi_max_port = desired_midi_max_port;

        let ui_config_clone = ui_config.clone();
        std::thread::spawn(move || {
            tracing::info!("LGS (GPU): 后台线程开始初始化");

            let lgs_result = Self::init_lgs_blocking(&ui_config_clone, desired_midi_max_port);

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
    fn init_lgs_blocking(ui_config: &UiConfig, midi_max_port: u8) -> MidiInitResult {
        use lumino_midi_io::ApiKind;

        let path = PathBuf::from(&ui_config.soundfont_path);
        let api_kind = ApiKind::Lgs {
            soundfont_path: path,
            sample_rate: ui_config.lgs_sample_rate,
            block_size: ui_config.lgs_block_size,
            max_voices_per_key: ui_config.lgs_max_voices_per_key,
            // REND-016 #139：全局复音上限透传（None = 自动 16384）
            max_voices: resolve_lgs_max_voices(ui_config.lgs_global_voice_limit),
            // REND-016 #139：防爆闸（发送端软 NPS 闸）开关
            soft_nps_gate: ui_config.lgs_soft_nps_gate,
            use_sinc: ui_config.lgs_use_sinc,
            velocity_filter_threshold: ui_config.lgs_velocity_filter_threshold,
            audio_output_device: ui_config.audio_output_device.clone(),
            // REND-002：文档端口布局（0 = 单端口/16 通道）
            midi_max_port,
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

/// REND-016 #139：解析 LGS 全局复音上限。
///
/// 配置 `None` = 自动（[`LGS_AUTO_MAX_VOICES`] = 16384）；显式值夹紧到
/// [64, 1_000_000]（上限与 GPU `SynthConfig::validate` 一致，防止手改配置
/// 产生无效值导致 GPU 初始化失败）。
fn resolve_lgs_max_voices(configured: Option<usize>) -> usize {
    configured
        .unwrap_or(LGS_AUTO_MAX_VOICES)
        .clamp(64, 1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REND-016 #139：自动 / 显式 / 越界三种配置的解析口径。
    #[test]
    fn test_resolve_lgs_max_voices_covers_auto_and_clamp() {
        assert_eq!(resolve_lgs_max_voices(None), LGS_AUTO_MAX_VOICES);
        assert_eq!(resolve_lgs_max_voices(Some(8192)), 8192);
        assert_eq!(
            resolve_lgs_max_voices(Some(0)),
            64,
            "0 视为非法下界，夹紧到 64"
        );
        assert_eq!(
            resolve_lgs_max_voices(Some(usize::MAX)),
            1_000_000,
            "上限对齐 SynthConfig::validate"
        );
    }
}
