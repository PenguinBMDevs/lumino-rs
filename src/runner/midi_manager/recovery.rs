//! 音频流恢复与后端重初始化
//!
//! 从 `midi_manager.rs` 零逻辑变更拆分而来。

use super::*;
use lumino_core::{tr, trf};

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

    /// 检查异步初始化是否已完成（含成功/失败两种落定）
    ///
    /// 返回 [`AsyncInitOutcome`]：`Switched` 表示后端已成功切换；`Failed`
    /// 表示初始化失败/线程断开、保持当前后端。两者都必须重连播放输出
    /// （#127：失败分支此前不重连，播放引擎停留在已被释放的旧连接上 → 无声）。
    pub fn check_async_init_complete(&mut self) -> AsyncInitOutcome {
        if self.is_xsynth_initializing {
            self.finish_xsynth_init()
        } else if self.is_lgs_initializing {
            self.finish_lgs_init()
        } else {
            AsyncInitOutcome::Pending
        }
    }

    /// 处理 XSynth 异步初始化结果（非阻塞）
    fn finish_xsynth_init(&mut self) -> AsyncInitOutcome {
        let rx = match &self.xsynth_init_rx {
            Some(rx) => rx,
            None => return AsyncInitOutcome::Pending,
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

                // REND-002 / DEBT-05 #122：初始化期间文档可能已装载/切换，完成时
                // 对齐端口布局；重建较慢，统一下发后台防抖 worker（不卡 UI）。
                if layout::should_align_after_init(
                    self.desired_midi_max_port,
                    self.spawned_midi_max_port,
                ) {
                    let desired = self.desired_midi_max_port;
                    tracing::info!("XSynth: 初始化后端口布局对齐 max_port={desired}（后台重建）");
                    self.apply_midi_port_layout_deferred(desired);
                }

                AsyncInitOutcome::Switched
            }
            Ok(XSynthInitResult::Failed(e)) => {
                tracing::warn!("XSynth: 异步初始化失败: {}", e);
                self.is_xsynth_initializing = false;
                self.xsynth_init_rx = None;
                // 返工 #149：启动时未建 System 占位——失败时才初始化 System 回退
                self.ensure_system_fallback_initialized();
                AsyncInitOutcome::Failed
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // 还在初始化中，不做任何事
                AsyncInitOutcome::Pending
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("XSynth: 初始化线程异常断开");
                self.is_xsynth_initializing = false;
                self.xsynth_init_rx = None;
                self.ensure_system_fallback_initialized();
                AsyncInitOutcome::Failed
            }
        }
    }

    /// 处理 LGS (GPU) 异步初始化结果（非阻塞）
    fn finish_lgs_init(&mut self) -> AsyncInitOutcome {
        let rx = match &self.lgs_init_rx {
            Some(rx) => rx,
            None => return AsyncInitOutcome::Pending,
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

                // REND-002 / DEBT-05 #122：初始化期间文档可能已装载/切换，完成时
                // 对齐端口布局（后台防抖重建，同 XSynth 口径）。
                if layout::should_align_after_init(
                    self.desired_midi_max_port,
                    self.spawned_midi_max_port,
                ) {
                    let desired = self.desired_midi_max_port;
                    tracing::info!(
                        "LGS (GPU): 初始化后端口布局对齐 max_port={desired}（后台重建）"
                    );
                    self.apply_midi_port_layout_deferred(desired);
                }

                AsyncInitOutcome::Switched
            }
            Ok(LgsInitResult::Failed(e)) => {
                tracing::warn!("LGS (GPU): 异步初始化失败: {}", e);
                self.is_lgs_initializing = false;
                self.lgs_init_rx = None;
                // #127 问题1：失败不得静默——生成可见提示（原因随提示展示）。
                self.pending_backend_notice =
                    Some(trf!("LGS 初始化失败，已回退 System 后端：{e}", e = e));
                // 返工 #149：启动时未建 System 占位——失败时才初始化 System 回退
                self.ensure_system_fallback_initialized();
                AsyncInitOutcome::Failed
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => AsyncInitOutcome::Pending,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("LGS (GPU): 初始化线程异常断开");
                self.is_lgs_initializing = false;
                self.lgs_init_rx = None;
                self.pending_backend_notice =
                    Some(tr!("LGS 初始化线程异常断开，已回退 System 后端").to_string());
                self.ensure_system_fallback_initialized();
                AsyncInitOutcome::Failed
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

    /// 返工 #149：软件合成后端（XSynth/LGS）是否正在异步初始化。
    ///
    /// 在途期间**禁止任何 System(WinMM) 输出创建**（占位连接与播放输出均
    /// 延迟到就绪/失败后由既有重连路径接线）：omv2 等环境下 WinMM 接口
    /// 可能在原生层崩溃（AV），启动路径必须零触碰。
    pub fn software_backend_pending(&self) -> bool {
        self.is_xsynth_initializing || self.is_lgs_initializing
    }

    /// 返工 #149：软件后端失败/断开时，若当前无任何后端则初始化 System 回退。
    ///
    /// 启动时不再建 System 占位（零 WinMM 触碰），故失败路径必须在此显式兜底；
    /// `pending_backend_notice` 已有内容（如 LGS/XSynth 的失败原因）时不覆盖。
    pub(super) fn ensure_system_fallback_initialized(&mut self) {
        if self.api.is_some() {
            return;
        }
        let system_result = Self::init_system_output(self.winmm_output_device_id);
        self.api = system_result.api;
        self.output = system_result.output;
        self.active_backend = system_result.backend;
        if self.pending_backend_notice.is_none() {
            self.pending_backend_notice = system_result.notice;
        }
    }

    /// 如果设置改变，重新初始化 MIDI 输出
    pub fn reinit_if_needed(&mut self, ui_config: &UiConfig) {
        if !self.needs_reinit {
            return;
        }

        self.needs_reinit = false;

        // DEBT-05 #122：在途后台布局重建先收尾归还 API，再重建整个后端。
        self.drain_layout_apply();

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
                // 返工 #149：软件后端初始化期间不触碰 System(WinMM)——不再做 System
                // 快速启动占位；仅当异步未启动（提前 bail：无音色库/文件缺失）时立即回退。
                self.api = None;
                self.output = None;
                self.active_backend = SynthBackend::System;
                if !self.start_xsynth_async_init(ui_config) {
                    self.ensure_system_fallback_initialized();
                }
            }
            SynthBackend::Lgs => {
                // 返工 #149：同上（LGS 异步在途期间零 WinMM 触碰）
                self.api = None;
                self.output = None;
                self.active_backend = SynthBackend::System;
                if !self.start_lgs_async_init(ui_config) {
                    self.ensure_system_fallback_initialized();
                }
            }
            SynthBackend::Kdmapi => {
                let backend_result = Self::init_kdmapi_output(self.winmm_output_device_id);
                self.api = backend_result.api;
                self.output = backend_result.output;
                self.active_backend = backend_result.backend;
                // 失败回退 System 时给出可见原因（成功为 None，无副作用）
                self.pending_backend_notice = backend_result.notice;
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
