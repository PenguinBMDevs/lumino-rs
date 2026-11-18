//! Maestro 后端异步初始化（AUD-009）。
//!
//! `Maestro::new` 会加载音色库并启动音频流（本机首次约 950ms），禁止在 UI
//! 线程同步执行；沿用 XSynth/LGS 的“后台线程 + 结果通道”模式。

use std::sync::mpsc::channel;

use lumino_midi_io::Api;
use lumino_midi_io::api::maestro::{APP_ID, Maestro, MaestroStatsReader};

use super::*;

impl MidiManager {
    /// 启动 Maestro 异步初始化。
    ///
    /// 返回 `true` = 已接管启动路径；返回 `false` = 平台不支持（Android），
    /// 调用方应立即回退 System。
    pub(super) fn start_maestro_async_init(&mut self, _ui_config: &UiConfig) -> bool {
        if self.is_maestro_initializing {
            return true;
        }

        if cfg!(target_os = "android") {
            self.pending_backend_notice =
                Some("Maestro 后端在 Android 上不可用，已回退 System".to_string());
            return false;
        }

        tracing::info!("Maestro: 启动后台初始化...");
        self.is_maestro_initializing = true;
        let desired_midi_max_port = self.desired_midi_max_port;
        self.spawned_midi_max_port = desired_midi_max_port;

        let (tx, rx) = channel();
        self.maestro_init_rx = Some(rx);

        std::thread::spawn(move || {
            let result = init_maestro_blocking(desired_midi_max_port);
            let init_result = match result {
                Ok((api, output, stats)) => MaestroInitResult::Success { api, output, stats },
                Err(error) => MaestroInitResult::Failed(error),
            };
            let _ = tx.send(init_result);
        });

        true
    }
}

fn init_maestro_blocking(
    midi_max_port: u8,
) -> Result<(MidiApi, MidiOutput, MaestroStatsReader), String> {
    let maestro = Maestro::new(APP_ID, midi_max_port)
        .map_err(|error| format!("Maestro 初始化失败: {error}"))?;
    let stats = maestro.stats_reader();
    let output = maestro
        .open_output(0)
        .map_err(|error| format!("Maestro 无法打开输出: {error}"))?;
    let api: MidiApi = Box::new(maestro);
    Ok((api, output, stats))
}
