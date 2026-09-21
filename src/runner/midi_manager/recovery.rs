//! 音频流恢复与后端重初始化
//!
//! 从 `midi_manager.rs` 零逻辑变更拆分而来。

use super::*;

impl MidiManager {
    /// 处理音频流恢复（音频设备被拔出/更换后）。
    ///
    /// XSynth 底层会自动重定向到系统默认输出设备；仅当自愈失败
    /// （如新设备参数与管线不一致）时，此方法触发上层重建管线。
    pub fn handle_stream_recovery(&mut self) {
        let Some(api) = self.api.as_mut() else {
            return;
        };
        if !api.poll_stream_recovery_needed() {
            return;
        }
        tracing::warn!("音频设备已改变，正在恢复音频流...");
        match api.recover_stream() {
            Ok(()) => tracing::info!("音频流已恢复（已重定向/重建到默认输出设备）"),
            Err(e) => tracing::error!("音频流恢复失败: {e}"),
        }
    }

    /// 检查异步初始化是否完成，如果完成则切换到 XSynth
    ///
    /// 返回 `true` 表示后端已成功切换到 XSynth，调用方应据此更新播放 MIDI 输出。
    pub fn check_async_init_complete(&mut self) -> bool {
        if self.is_xsynth_initializing {
            self.finish_xsynth_init()
        } else if self.is_lgs_initializing {
            self.finish_lgs_init()
        } else {
            false
        }
    }

    /// 处理 XSynth 异步初始化结果（非阻塞）
    fn finish_xsynth_init(&mut self) -> bool {
        let rx = match &self.xsynth_init_rx {
            Some(rx) => rx,
            None => return false,
        };

        // 非阻塞检查接收器
        match rx.try_recv() {
            Ok(XSynthInitResult::Success { api, output }) => {
                tracing::info!("XSynth: 异步初始化完成，切换到 XSynth 后端");

                // 关闭旧的输出
                if let Some(old_output) = self.output.take() {
                    drop(old_output);
                }

                self.api = Some(api);
                self.output = Some(output);
                self.active_backend = SynthBackend::XSynth;
                self.is_xsynth_initializing = false;
                self.xsynth_init_rx = None;

                true
            }
            Ok(XSynthInitResult::Failed(e)) => {
                tracing::warn!("XSynth: 异步初始化失败: {}", e);
                self.is_xsynth_initializing = false;
                self.xsynth_init_rx = None;
                // 保持在当前后端（System）
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // 还在初始化中，不做任何事
                false
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("XSynth: 初始化线程异常断开");
                self.is_xsynth_initializing = false;
                self.xsynth_init_rx = None;
                false
            }
        }
    }

    /// 处理 LGS (GPU) 异步初始化结果（非阻塞）
    fn finish_lgs_init(&mut self) -> bool {
        let rx = match &self.lgs_init_rx {
            Some(rx) => rx,
            None => return false,
        };

        match rx.try_recv() {
            Ok(LgsInitResult::Success { api, output }) => {
                tracing::info!("LGS (GPU): 异步初始化完成，切换到 LGS (GPU) 后端");

                // 关闭旧的输出
                if let Some(old_output) = self.output.take() {
                    drop(old_output);
                }

                self.api = Some(api);
                self.output = Some(output);
                self.active_backend = SynthBackend::Lgs;
                self.is_lgs_initializing = false;
                self.lgs_init_rx = None;

                true
            }
            Ok(LgsInitResult::Failed(e)) => {
                tracing::warn!("LGS (GPU): 异步初始化失败: {}", e);
                self.is_lgs_initializing = false;
                self.lgs_init_rx = None;
                // 保持在当前后端（System）
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("LGS (GPU): 初始化线程异常断开");
                self.is_lgs_initializing = false;
                self.lgs_init_rx = None;
                false
            }
        }
    }

    /// 标记需要重新初始化
    pub fn mark_for_reinit(&mut self) {
        self.needs_reinit = true;
    }

    /// 检查是否需要重新初始化
    pub fn needs_reinit(&self) -> bool {
        self.needs_reinit
    }

    /// 是否正在进行 XSynth 异步初始化
    ///
    /// 用于在 `reinit_if_needed` 之后判断新后端是「立即就绪的同步后端」
    /// （Core / System / Kdmapi）还是「仍在异步初始化的 XSynth-Realtime」，
    /// 从而决定是否需要立即把播放输出重连到新连接（同步后端必须立即重连，
    /// 否则 PlaybackManager 继续向已被丢弃的旧连接发送事件 → 切换后无声）。
    pub fn is_xsynth_initializing(&self) -> bool {
        self.is_xsynth_initializing
    }

    /// 是否正在进行 LGS (GPU) 异步初始化
    ///
    /// 语义同 [`Self::is_xsynth_initializing`]，用于 `reinit_if_needed` 后判断
    /// 新后端是同步就绪还是仍在异步初始化。
    pub fn is_lgs_initializing(&self) -> bool {
        self.is_lgs_initializing
    }

    /// 如果设置改变，重新初始化 MIDI 输出
    pub fn reinit_if_needed(&mut self, ui_config: &UiConfig) {
        if !self.needs_reinit {
            return;
        }

        self.needs_reinit = false;

        tracing::info!(
            "重新初始化 MIDI 输出，使用偏好后端: {:?}",
            ui_config.preferred_backend
        );

        // 更新偏好后端
        self.preferred_backend = ui_config.preferred_backend;

        // 更新 XSynth 配置（供 create_additional_output 回退创建时使用）
        self.xsynth_soundfont_path = ui_config.soundfont_path.clone();
        self.xsynth_buffer_ms = ui_config.xsynth_buffer_ms;
        self.xsynth_sample_rate = ui_config.xsynth_sample_rate;
        self.xsynth_max_voices_per_key = ui_config.xsynth_max_voices_per_key;
        self.xsynth_global_voice_limit = ui_config.xsynth_global_voice_limit;
        self.xsynth_voice_target_ratio = ui_config.xsynth_voice_target_ratio;
        self.xsynth_soft_nps_gate = ui_config.xsynth_soft_nps_gate;

        // 更新 LGS (GPU) 配置（供异步初始化使用）
        self.lgs_soundfont_path = ui_config.soundfont_path.clone();
        self.lgs_sample_rate = ui_config.lgs_sample_rate;
        self.lgs_block_size = ui_config.lgs_block_size;
        self.lgs_max_voices_per_key = ui_config.lgs_max_voices_per_key;
        self.lgs_use_sinc = ui_config.lgs_use_sinc;

        // 更新指定的 WinMM 播表（输出设备）ID
        self.winmm_output_device_id = ui_config.system_output_device_id;

        // 清空 SoundFont 缓存，防止旧条目无限累积（每个 SF2 30-300MB）
        lumino_midi_io::soundfont_cache::clear_cache();

        // 关闭旧的 MIDI 输出和备用 API
        if let Some(old_output) = self.output.take() {
            drop(old_output);
        }
        self.fallback_api = None;
        self.xsynth_init_rx = None;
        self.is_xsynth_initializing = false;
        self.lgs_init_rx = None;
        self.is_lgs_initializing = false;

        // 重新初始化
        match ui_config.preferred_backend {
            SynthBackend::XSynth => {
                // 先快速启动 System，然后后台初始化 XSynth（Realtime 引擎）
                let system_result = Self::init_system_output(self.winmm_output_device_id);
                self.api = system_result.api;
                self.output = system_result.output;
                self.active_backend = system_result.backend;
                self.start_xsynth_async_init(ui_config);
            }
            SynthBackend::Lgs => {
                // 先快速启动 System，然后后台初始化 LGS (GPU)（Realtime 引擎）
                let system_result = Self::init_system_output(self.winmm_output_device_id);
                self.api = system_result.api;
                self.output = system_result.output;
                self.active_backend = system_result.backend;
                self.start_lgs_async_init(ui_config);
            }
            SynthBackend::Kdmapi => {
                let backend_result = Self::init_kdmapi_output(self.winmm_output_device_id);
                self.api = backend_result.api;
                self.output = backend_result.output;
                self.active_backend = backend_result.backend;
            }
            SynthBackend::System => {
                let backend_result = Self::init_system_output(self.winmm_output_device_id);
                self.api = backend_result.api;
                self.output = backend_result.output;
                self.active_backend = backend_result.backend;
            }
        }

        tracing::info!("MIDI 输出已重新初始化，实际后端: {:?}", self.active_backend);
    }
}
