//! MIDI 输出初始化与连接创建
//!
//! 从 `midi_manager.rs` 零逻辑变更拆分而来。

use super::*;

impl MidiManager {
    /// 从配置初始化 MIDI 管理器
    ///
    /// 如果配置使用 XSynth，会先使用 System 快速启动，然后在后台初始化 XSynth
    pub fn from_config(ui_config: &UiConfig) -> Self {
        let preferred = ui_config.preferred_backend;

        // 快速启动初始后端（不阻塞 UI）
        let init_result = match preferred {
            SynthBackend::Kdmapi => Self::init_kdmapi_output(ui_config.system_output_device_id),
            SynthBackend::System => Self::init_system_output(ui_config.system_output_device_id),
            SynthBackend::XSynth => Self::init_system_output(ui_config.system_output_device_id),
            SynthBackend::Lgs => Self::init_system_output(ui_config.system_output_device_id),
        };

        let mut manager = Self {
            api: init_result.api,
            fallback_api: None,
            output: init_result.output,
            active_backend: init_result.backend,
            needs_reinit: false,
            preferred_backend: preferred,
            xsynth_init_rx: None,
            is_xsynth_initializing: false,
            xsynth_soundfont_path: ui_config.soundfont_path.clone(),
            xsynth_buffer_ms: ui_config.xsynth_buffer_ms,
            xsynth_sample_rate: ui_config.xsynth_sample_rate,
            xsynth_max_voices_per_key: ui_config.xsynth_max_voices_per_key,
            xsynth_global_voice_limit: ui_config.xsynth_global_voice_limit,
            xsynth_voice_target_ratio: ui_config.xsynth_voice_target_ratio,
            xsynth_soft_nps_gate: ui_config.xsynth_soft_nps_gate,
            lgs_init_rx: None,
            is_lgs_initializing: false,
            lgs_soundfont_path: ui_config.soundfont_path.clone(),
            lgs_sample_rate: ui_config.lgs_sample_rate,
            lgs_block_size: ui_config.lgs_block_size,
            lgs_max_voices_per_key: ui_config.lgs_max_voices_per_key,
            lgs_use_sinc: ui_config.lgs_use_sinc,
            winmm_output_device_id: ui_config.system_output_device_id,
        };

        // 如果偏好 XSynth，在后台异步初始化（Core 已同步完成）
        if preferred == SynthBackend::XSynth && ui_config.audio_engine == AudioEngineKind::Realtime
        {
            manager.start_xsynth_async_init(ui_config);
        }

        // 如果偏好 LGS (GPU)，同样在后台异步初始化（GPU 设备创建 + 音色库加载较慢）
        if preferred == SynthBackend::Lgs && ui_config.audio_engine == AudioEngineKind::Realtime {
            manager.start_lgs_async_init(ui_config);
        }

        manager
    }

    /// 快速初始化 System 后端（不阻塞）
    ///
    /// `selected_device` 为系统播表（WinMM 输出设备）的指定 ID；
    /// 为 `None` 或该设备不存在时，回落到第一个输出设备（系统默认）。
    pub(super) fn init_system_output(selected_device: Option<u32>) -> BackendInitResult {
        use lumino_midi_io::ApiKind;

        tracing::info!("MIDI: 快速启动 System 后端");

        match lumino_midi_io::new_api(&ApiKind::System) {
            Ok(api) => {
                if let Ok(outputs) = api.outputs() {
                    // 指定的 WINMM 播表优先，否则使用第一个（系统默认）
                    let output = selected_device
                        .and_then(|id| outputs.iter().find(|o| o.id == id))
                        .or_else(|| outputs.first());
                    if let Some(output) = output
                        && let Ok(conn) = api.open_output(output.id)
                    {
                        tracing::info!("MIDI: System 后端已就绪 (输出设备 #{})", output.id);
                        return BackendInitResult {
                            api: Some(api),
                            output: Some(conn),
                            backend: SynthBackend::System,
                        };
                    }
                }
                BackendInitResult {
                    api: Some(api),
                    output: None,
                    backend: SynthBackend::System,
                }
            }
            Err(e) => {
                tracing::warn!("MIDI: System 后端启动失败: {:?}", e);
                BackendInitResult {
                    api: None,
                    output: None,
                    backend: SynthBackend::System,
                }
            }
        }
    }

    /// 快速初始化 KDMAPI 后端（不阻塞）
    ///
    /// 支持多路径自动搜索（详见 `kdmapi.rs` 的 `find_omnimidi_paths`）：
    /// 1. 当前目录 / DLL 搜索路径
    /// 2. `%WINDIR%\System32\OmniMIDI\OmniMIDI.dll`（标准安装路径）
    /// 3. `%PROGRAMFILES%\OmniMIDI\OmniMIDI.dll`
    ///
    /// 如果 KDMAPI 初始化失败，会自动回退到 System 后端，保证至少能出声。
    pub(super) fn init_kdmapi_output(selected_device: Option<u32>) -> BackendInitResult {
        use lumino_midi_io::ApiKind;

        tracing::info!("MIDI: 尝试启动 KDMAPI 后端");

        let path = std::path::PathBuf::from("OmniMIDI.dll");

        match lumino_midi_io::new_api(&ApiKind::Kdmapi { path }) {
            Ok(api) => {
                if let Ok(outputs) = api.outputs()
                    && let Some(output) = outputs.first()
                    && let Ok(conn) = api.open_output(output.id)
                {
                    tracing::info!("MIDI: KDMAPI 后端已就绪");
                    return BackendInitResult {
                        api: Some(api),
                        output: Some(conn),
                        backend: SynthBackend::Kdmapi,
                    };
                }
                tracing::warn!("MIDI: KDMAPI 已初始化但无法打开输出，回退到 System 后端");
                // 有 api 但无 output → 回退到 System 后端
                Self::init_system_output(selected_device)
            }
            Err(e) => {
                tracing::warn!("MIDI: KDMAPI 后端启动失败: {:?}，回退到 System 后端", e);
                // KDMAPI 完全不可用 → 回退到 System 后端
                Self::init_system_output(selected_device)
            }
        }
    }

    /// 获取 MIDI 输出连接的可变引用
    pub fn output_mut(&mut self) -> Option<&mut Box<dyn lumino_midi_io::OutputConnection>> {
        self.output.as_mut()
    }

    /// 创建额外的 MIDI 输出连接（用于播放引擎）
    ///
    /// 使用多策略 fallback:
    /// 1. 在现有 API 上打开第二个连接（某些驱动可能不支持）
    /// 2. 创建全新 API 实例 + 连接（保存新 API 到 fallback_api 防止释放）
    /// 3. 兜底：取走主输出连接（播放期间音符预览静音，但至少播放功能正常）
    pub fn create_additional_output(
        &mut self,
    ) -> Option<Box<dyn lumino_midi_io::OutputConnection>> {
        // ── 策略1：在现有 API 上尝试打开第二个连接 ──
        if let Some(api) = self.api.as_ref()
            && let Ok(outputs) = api.outputs()
            && let Some(output) = outputs.first()
            && let Ok(conn) = api.open_output(output.id)
        {
            tracing::info!("MIDI 播放输出: 策略1成功，从现有 API 创建了第二个连接");
            return Some(conn);
        }

        // ── 策略2：创建全新的 API 实例 ──
        // OutputConnection 是自包含的，不需要父 Api 保持存活
        // （midir::MidiOutputConnection 持有自己的 OS 句柄）
        let strategy2_result = match self.active_backend {
            SynthBackend::XSynth | SynthBackend::Lgs if self.api.is_some() => {
                // XSynth：禁止创建第二个实例！
                // 策略1（共享 sender）总是成功；如果策略1失败说明 API 已损坏。
                // 创建第二个 RealtimeSynth 会导致：
                // 1. 双份 cpal 音频流（双倍的 CPU/内存开销）
                // 2. 声音叠加（同一音符被两个合成器同时播放）
                // 3. 音色库重复加载（30-300MB 内存浪费）
                tracing::error!(
                    "MIDI 播放输出: XSynth 策略1意外失败，拒绝创建第二个实例。\
                     当前后端状态异常，建议检查 XSynth 初始化或重新启动应用"
                );
                None
            }
            // Core 引擎：active_backend 同为 XSynth，但不持有独立 API 实例，
            // 不能按「第二个实例」处理，直接回退到策略3（复用主输出）。
            SynthBackend::XSynth | SynthBackend::Lgs => None,
            SynthBackend::System | SynthBackend::Kdmapi => {
                let api_kind = match self.active_backend {
                    SynthBackend::Kdmapi => {
                        let path = std::path::PathBuf::from("OmniMIDI.dll");
                        lumino_midi_io::ApiKind::Kdmapi { path }
                    }
                    _ => lumino_midi_io::ApiKind::System,
                };
                Self::try_open_new_api(&api_kind, None)
            }
        };

        if let Some((new_api, conn)) = strategy2_result {
            // 必须保持 new_api 存活，否则连接可能失效
            self.fallback_api = Some(new_api);
            tracing::info!("MIDI 播放输出: 策略2成功，从全新 API 实例创建了连接");
            return Some(conn);
        }

        // ── 策略3：兜底——取走主输出连接给播放引擎 ──
        // 播放期间音符预览会暂时无响应，但播放功能正常
        // System 后端启动时只有 1 个 MIDI OUT 端口，fallback 是预期行为，不记录日志
        if let Some(output) = self.output.take() {
            // 仅真正的 XSynth-Realtime 还 fallback 才值得警告；
            // Core 引擎本就复用主输出，属于预期行为，不报警。
            if matches!(
                self.active_backend,
                SynthBackend::XSynth | SynthBackend::Lgs
            ) && self.api.is_some()
            {
                tracing::warn!(
                    "MIDI 播放输出: 策略1和2均失败，使用主输出作为播放输出（音符预览将暂时不可用）"
                );
            }
            return Some(output);
        }

        tracing::error!("MIDI 播放输出: 无法创建任何输出连接，播放将无声");
        None
    }

    /// 辅助方法：尝试创建新的 API 实例并打开输出连接
    ///
    /// 返回 `(api, connection)` 元组，其中 `api` 需要保持存活。
    fn try_open_new_api(
        api_kind: &lumino_midi_io::ApiKind,
        options: Option<lumino_midi_io::api::xsynth::XSynthOptions>,
    ) -> Option<(
        Box<dyn lumino_midi_io::Api>,
        Box<dyn lumino_midi_io::OutputConnection>,
    )> {
        let new_api: Box<dyn lumino_midi_io::Api> = match options {
            Some(opts) => lumino_midi_io::new_api_with_options(api_kind, Some(opts)).ok()?,
            None => lumino_midi_io::new_api(api_kind).ok()?,
        };

        let outputs = new_api.outputs().ok()?;
        let output = outputs.first()?;
        let conn = new_api.open_output(output.id).ok()?;

        Some((new_api, conn))
    }

    /// 创建独立的 MIDI 输入 API（用于录制功能）
    ///
    /// 返回一个新的 API 实例，供 UI 层独立管理输入设备的生命周期。
    /// 对于不支持输入的 XSynth 后端，返回 System 后端的输入 API。
    pub fn create_input_api(&self) -> Option<Box<dyn lumino_midi_io::Api>> {
        let api_kind = match self.active_backend {
            SynthBackend::XSynth | SynthBackend::Lgs => {
                tracing::info!(
                    "MIDI 输入 API: {:?} 不支持输入，使用 System 后端",
                    self.active_backend
                );
                lumino_midi_io::ApiKind::System
            }
            SynthBackend::Kdmapi => {
                let path = std::path::PathBuf::from("OmniMIDI.dll");
                lumino_midi_io::ApiKind::Kdmapi { path }
            }
            SynthBackend::System => lumino_midi_io::ApiKind::System,
        };

        match lumino_midi_io::new_api(&api_kind) {
            Ok(api) => {
                tracing::info!("MIDI 输入 API: 已创建 (backend={:?})", self.active_backend);
                Some(api)
            }
            Err(e) => {
                tracing::error!("MIDI 输入 API: 创建失败: {:?}", e);
                None
            }
        }
    }
}
