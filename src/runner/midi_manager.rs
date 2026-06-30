use lumino_audio::{AudioCommandAdapter, CpalAudioHandle, spawn_cpal_audio};
use lumino_core::storage::config::{SynthBackend, UiConfig};
use std::path::PathBuf;

/// MIDI API 类型别名
type MidiApi = Box<dyn lumino_midi_io::Api>;
/// MIDI 输出连接类型别名
type MidiOutput = Box<dyn lumino_midi_io::OutputConnection>;

/// MIDI 设备管理器
///
/// 负责管理 MIDI API 和输出连接的生命周期。
/// XSynth 后端使用新的 lumino-audio 引擎（ring buffer + 同步渲染），
/// KDMAPI/System 后端使用旧的 lumino_midi_io 路径。
pub struct MidiManager {
    /// 新音频引擎句柄（仅 XSynth 后端）
    audio_handle: Option<CpalAudioHandle>,
    /// 主 MIDI 输出连接（预览音符用）
    /// XSynth: AudioCommandAdapter；KDMAPI/System: 原生 OutputConnection
    output: Option<MidiOutput>,
    /// API 实例（用于保持合成器存活，仅 KDMAPI/System）
    api: Option<MidiApi>,
    /// 备用 API 实例（用于 create_additional_output 创建独立播放连接时保持存活）
    fallback_api: Option<MidiApi>,
    /// 实际启用的合成器后端
    active_backend: SynthBackend,
    /// 是否需要重新初始化
    needs_reinit: bool,
    /// 配置中偏好的后端
    preferred_backend: SynthBackend,
    /// XSynth 音色库路径
    soundfont_path: String,
    /// XSynth 采样率
    sample_rate: u32,
    /// cmd_tx 的克隆（用于创建额外的 OutputConnection adapter）
    cmd_tx_clone: Option<crossbeam_channel::Sender<lumino_audio::AudioCommand>>,
}

impl Default for MidiManager {
    fn default() -> Self {
        Self {
            audio_handle: None,
            output: None,
            api: None,
            fallback_api: None,
            active_backend: SynthBackend::System,
            needs_reinit: false,
            preferred_backend: SynthBackend::System,
            soundfont_path: String::new(),
            sample_rate: 44100,
            cmd_tx_clone: None,
        }
    }
}

impl MidiManager {
    /// 从配置初始化 MIDI 管理器
    pub fn from_config(ui_config: &UiConfig) -> Self {
        let preferred = ui_config.preferred_backend;

        match preferred {
            SynthBackend::XSynth => Self::init_xsynth(ui_config),
            SynthBackend::Kdmapi => Self::init_kdmapi(),
            SynthBackend::System => Self::init_system(),
        }
    }

    /// 初始化 XSynth 后端（使用新的 lumino-audio 引擎）
    fn init_xsynth(ui_config: &UiConfig) -> Self {
        let sample_rate = if ui_config.xsynth_sample_rate > 0 {
            ui_config.xsynth_sample_rate
        } else {
            44100
        };

        tracing::info!(
            "XSynth: 启动新音频引擎 (lumino-audio), 采样率={}Hz",
            sample_rate
        );

        match spawn_cpal_audio(sample_rate) {
            Ok((handle, _engine)) => {
                let cmd_tx = handle.cmd_tx.clone();
                let adapter = Box::new(AudioCommandAdapter::new(cmd_tx.clone()));

                tracing::info!("XSynth: 新音频引擎已就绪");

                // 异步加载音色库
                if !ui_config.soundfont_path.is_empty() {
                    let path = PathBuf::from(&ui_config.soundfont_path);
                    if path.exists() {
                        tracing::info!("XSynth: 加载音色库 {:?}", path);
                        handle.load_model(
                            std::sync::Arc::new(lumino_midi_loader::MidiDocument {
                                notes: vec![],
                                control_events: vec![],
                                tempo_changes: vec![(0, 120.0)],
                                track_names: vec![],
                                total_ticks: 0,
                                track_count: 0,
                                tracks: lumino_midi_loader::TrackManager::new(0),
                            }),
                            vec![path],
                        );
                    } else {
                        tracing::warn!("XSynth: 音色库文件不存在: {:?}", path);
                    }
                }

                Self {
                    audio_handle: Some(handle),
                    output: Some(adapter),
                    api: None,
                    fallback_api: None,
                    active_backend: SynthBackend::XSynth,
                    needs_reinit: false,
                    preferred_backend: SynthBackend::XSynth,
                    soundfont_path: ui_config.soundfont_path.clone(),
                    sample_rate,
                    cmd_tx_clone: Some(cmd_tx),
                }
            }
            Err(e) => {
                tracing::error!("XSynth: 新音频引擎启动失败: {}，回退到 System 后端", e);
                let system = Self::init_system();
                Self {
                    preferred_backend: SynthBackend::XSynth,
                    soundfont_path: ui_config.soundfont_path.clone(),
                    sample_rate,
                    ..system
                }
            }
        }
    }

    /// 快速初始化 System 后端
    fn init_system() -> Self {
        use lumino_midi_io::ApiKind;

        tracing::info!("MIDI: 启动 System 后端");

        match lumino_midi_io::new_api(&ApiKind::System) {
            Ok(api) => {
                let output = api.outputs().ok()
                    .and_then(|outputs| outputs.first().cloned())
                    .and_then(|output| api.open_output(output.id).ok());

                Self {
                    audio_handle: None,
                    output,
                    api: Some(api),
                    fallback_api: None,
                    active_backend: SynthBackend::System,
                    needs_reinit: false,
                    preferred_backend: SynthBackend::System,
                    soundfont_path: String::new(),
                    sample_rate: 44100,
                    cmd_tx_clone: None,
                }
            }
            Err(e) => {
                tracing::warn!("MIDI: System 后端启动失败: {:?}", e);
                Self::default()
            }
        }
    }

    /// 快速初始化 KDMAPI 后端
    fn init_kdmapi() -> Self {
        use lumino_midi_io::ApiKind;

        tracing::info!("MIDI: 启动 KDMAPI 后端");

        let path = PathBuf::from("OmniMIDI.dll");
        match lumino_midi_io::new_api(&ApiKind::Kdmapi { path }) {
            Ok(api) => {
                let output = api.outputs().ok()
                    .and_then(|outputs| outputs.first().cloned())
                    .and_then(|output| api.open_output(output.id).ok());

                Self {
                    audio_handle: None,
                    output,
                    api: Some(api),
                    fallback_api: None,
                    active_backend: SynthBackend::Kdmapi,
                    needs_reinit: false,
                    preferred_backend: SynthBackend::Kdmapi,
                    soundfont_path: String::new(),
                    sample_rate: 44100,
                    cmd_tx_clone: None,
                }
            }
            Err(e) => {
                tracing::warn!("MIDI: KDMAPI 后端启动失败: {:?}", e);
                Self::default()
            }
        }
    }

    /// 获取 MIDI 输出连接的可变引用
    pub fn output_mut(&mut self) -> Option<&mut MidiOutput> {
        self.output.as_mut()
    }

    /// 创建额外的 MIDI 输出连接（用于播放引擎）
    ///
    /// XSynth: 返回 AudioCommandAdapter（共享同一 cmd_tx）
    /// KDMAPI/System: 多策略 fallback
    pub fn create_additional_output(&mut self) -> Option<MidiOutput> {
        match self.active_backend {
            SynthBackend::XSynth => {
                // XSynth: 创建共享同一 cmd_tx 的 adapter
                self.cmd_tx_clone.as_ref().map(|tx| {
                    Box::new(AudioCommandAdapter::new(tx.clone())) as MidiOutput
                })
            }
            SynthBackend::System | SynthBackend::Kdmapi => {
                // ── 策略1：在现有 API 上尝试打开第二个连接 ──
                if let Some(api) = self.api.as_ref()
                    && let Ok(outputs) = api.outputs()
                    && let Some(output) = outputs.first()
                    && let Ok(conn) = api.open_output(output.id)
                {
                    tracing::info!("MIDI 播放输出: 策略1成功");
                    return Some(conn);
                }

                // ── 策略2：创建全新的 API 实例 ──
                let api_kind = match self.active_backend {
                    SynthBackend::Kdmapi => {
                        lumino_midi_io::ApiKind::Kdmapi {
                            path: PathBuf::from("OmniMIDI.dll"),
                        }
                    }
                    _ => lumino_midi_io::ApiKind::System,
                };

                if let Ok(new_api) = lumino_midi_io::new_api(&api_kind) {
                    if let Ok(outputs) = new_api.outputs()
                        && let Some(output) = outputs.first()
                        && let Ok(conn) = new_api.open_output(output.id)
                    {
                        self.fallback_api = Some(new_api);
                        tracing::info!("MIDI 播放输出: 策略2成功");
                        return Some(conn);
                    }
                }

                // ── 策略3：兜底——取走主输出连接 ──
                self.output.take().map(|output| {
                    tracing::warn!("MIDI 播放输出: 使用主输出作为播放输出");
                    output
                })
            }
        }
    }

    /// 创建独立的 MIDI 输入 API（用于录制功能）
    pub fn create_input_api(&self) -> Option<MidiApi> {
        let api_kind = match self.active_backend {
            SynthBackend::XSynth => {
                tracing::info!("MIDI 输入 API: XSynth 不支持输入，使用 System 后端");
                lumino_midi_io::ApiKind::System
            }
            SynthBackend::Kdmapi => {
                lumino_midi_io::ApiKind::Kdmapi {
                    path: PathBuf::from("OmniMIDI.dll"),
                }
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

    /// 标记需要重新初始化
    pub fn mark_for_reinit(&mut self) {
        self.needs_reinit = true;
    }

    /// 检查是否需要重新初始化
    pub fn needs_reinit(&self) -> bool {
        self.needs_reinit
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

        // 清空 SoundFont 缓存
        lumino_midi_io::soundfont_cache::clear_cache();

        // 关闭旧的输出和引擎
        self.output.take();
        self.audio_handle.take();
        self.api.take();
        self.fallback_api.take();
        self.cmd_tx_clone.take();

        // 用新配置重新初始化
        let new_manager = Self::from_config(ui_config);
        *self = new_manager;

        tracing::info!("MIDI 输出已重新初始化，实际后端: {:?}", self.active_backend);
    }

    /// XSynth 异步初始化是否完成（新引擎为同步初始化，总是返回 false）
    ///
    /// 保留此方法是为了兼容 lifecycle/midi.rs 的调用，
    /// 新引擎不需要异步初始化，直接返回 false。
    pub fn check_async_init_complete(&mut self) -> bool {
        false
    }
}

/// 处理音频动作（预览音符）
pub fn handle_audio_action(
    output: &mut MidiOutput,
    action: lumino_ui::message::AudioAction,
) {
    use lumino_ui::message::AudioAction;

    match action {
        AudioAction::PlayNote { key, velocity } => {
            tracing::debug!("Runner: 调用 output.note_on(0, {}, {})", key, velocity);
            if let Err(e) = output.note_on(0, key, velocity) {
                tracing::warn!("播放音符失败: {}", e);
            }
        }
        AudioAction::StopNote { key } => {
            tracing::debug!("Runner: 调用 output.note_off(0, {}, 0)", key);
            if let Err(e) = output.note_off(0, key, 0) {
                tracing::warn!("停止音符失败: {}", e);
            }
        }
    }
}
