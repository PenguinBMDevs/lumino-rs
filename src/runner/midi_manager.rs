use lumino_core::storage::config::{SynthBackend, UiConfig};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

/// MIDI API 类型别名
type MidiApi = Box<dyn lumino_midi_io::Api>;
/// MIDI 输出连接类型别名
type MidiOutput = Box<dyn lumino_midi_io::OutputConnection>;
/// MIDI 初始化结果类型别名
type MidiInitResult = Result<(MidiApi, MidiOutput), String>;

/// 后端初始化结果
struct BackendInitResult {
    /// API 实例（用于保持合成器存活）
    api: Option<Box<dyn lumino_midi_io::Api>>,
    /// MIDI 输出连接
    output: Option<Box<dyn lumino_midi_io::OutputConnection>>,
    /// 实际使用的后端类型
    backend: SynthBackend,
}

/// XSynth 异步初始化结果
enum XSynthInitResult {
    Success {
        api: Box<dyn lumino_midi_io::Api>,
        output: Box<dyn lumino_midi_io::OutputConnection>,
    },
    Failed(String),
}

/// MIDI 设备管理器
///
/// 负责管理 MIDI API 和输出连接的生命周期
pub struct MidiManager {
    /// 保存 API 实例（用于保持 RealtimeSynth 等存活）
    api: Option<Box<dyn lumino_midi_io::Api>>,
    /// 备用 API 实例（用于 create_additional_output 创建独立播放连接时保持存活）
    fallback_api: Option<Box<dyn lumino_midi_io::Api>>,
    /// MIDI 输出连接
    output: Option<Box<dyn lumino_midi_io::OutputConnection>>,
    /// 实际启用的合成器后端
    active_backend: SynthBackend,
    /// 是否需要重新初始化
    needs_reinit: bool,
    /// 配置中偏好的后端（用于异步初始化后知道应该切换到哪个后端）
    preferred_backend: SynthBackend,
    /// XSynth 异步初始化接收器
    xsynth_init_rx: Option<Receiver<XSynthInitResult>>,
    /// 是否正在异步初始化 XSynth
    is_xsynth_initializing: bool,
    /// XSynth 音色库路径（用于 create_additional_output 回退创建时重用）
    xsynth_soundfont_path: String,
    /// XSynth 缓冲区大小（毫秒）
    xsynth_buffer_ms: f64,
    /// XSynth 合成线程数
    xsynth_threads: i32,
    /// XSynth 采样率
    xsynth_sample_rate: u32,
    /// XSynth 是否启用 killing fade-out
    xsynth_fade_out_killing: bool,
    /// XSynth 每个键最大同音数
    xsynth_max_voices_per_key: Option<usize>,
    /// XSynth 全局最大并发 voice 数
    xsynth_global_voice_limit: Option<usize>,
}

impl Default for MidiManager {
    fn default() -> Self {
        Self {
            api: None,
            fallback_api: None,
            output: None,
            active_backend: SynthBackend::System,
            needs_reinit: false,
            preferred_backend: SynthBackend::System,
            xsynth_init_rx: None,
            is_xsynth_initializing: false,
            xsynth_soundfont_path: String::new(),
            xsynth_buffer_ms: 0.0,
            xsynth_threads: 0,
            xsynth_sample_rate: 0,
            xsynth_fade_out_killing: false,
            xsynth_max_voices_per_key: None,
            xsynth_global_voice_limit: None,
        }
    }
}

impl MidiManager {
    /// 从配置初始化 MIDI 管理器
    ///
    /// 如果配置使用 XSynth，会先使用 System 快速启动，然后在后台初始化 XSynth
    pub fn from_config(ui_config: &UiConfig) -> Self {
        let preferred = ui_config.preferred_backend;

        // 快速启动初始后端（不阻塞 UI）
        let init_result = match preferred {
            SynthBackend::Kdmapi => Self::init_kdmapi_output(),
            SynthBackend::System => Self::init_system_output(),
            // XSynth 模式下先启动 System，然后在后台初始化 XSynth
            SynthBackend::XSynth => Self::init_system_output(),
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
            xsynth_threads: ui_config.xsynth_threads,
            xsynth_sample_rate: ui_config.xsynth_sample_rate,
            xsynth_fade_out_killing: ui_config.xsynth_fade_out_killing,
            xsynth_max_voices_per_key: ui_config.xsynth_max_voices_per_key,
            xsynth_global_voice_limit: ui_config.xsynth_global_voice_limit,
        };

        // 如果偏好 XSynth，在后台异步初始化
        if preferred == SynthBackend::XSynth {
            manager.start_xsynth_async_init(ui_config);
        }

        manager
    }

    /// 启动 XSynth 异步初始化
    fn start_xsynth_async_init(&mut self, ui_config: &UiConfig) {
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
            threads: ui_config.xsynth_threads,
            sample_rate: ui_config.xsynth_sample_rate,
            fade_out_killing: ui_config.xsynth_fade_out_killing,
        };

        let api = lumino_midi_io::new_api_with_options(&api_kind, Some(options))
            .map_err(|e| format!("初始化 MIDI API 失败: {:?}", e))?;

        // 诊断：打印音频后端信息
        if let Some(version) = api.version() {
            tracing::info!("XSynth: 音频后端已初始化 (version: {})", version);
        }
        tracing::info!(
            "XSynth: 采样率={}Hz, buffer={}ms, 线程={}",
            ui_config.xsynth_sample_rate,
            ui_config.xsynth_buffer_ms,
            ui_config.xsynth_threads,
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

    /// 快速初始化 System 后端（不阻塞）
    fn init_system_output() -> BackendInitResult {
        use lumino_midi_io::ApiKind;

        tracing::info!("MIDI: 快速启动 System 后端");

        match lumino_midi_io::new_api(&ApiKind::System) {
            Ok(api) => {
                if let Ok(outputs) = api.outputs()
                    && let Some(output) = outputs.first()
                    && let Ok(conn) = api.open_output(output.id)
                {
                    tracing::info!("MIDI: System 后端已就绪");
                    return BackendInitResult {
                        api: Some(api),
                        output: Some(conn),
                        backend: SynthBackend::System,
                    };
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
    fn init_kdmapi_output() -> BackendInitResult {
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
                Self::init_system_output()
            }
            Err(e) => {
                tracing::warn!("MIDI: KDMAPI 后端启动失败: {:?}，回退到 System 后端", e);
                // KDMAPI 完全不可用 → 回退到 System 后端
                Self::init_system_output()
            }
        }
    }

    /// 检查异步初始化是否完成，如果完成则切换到 XSynth
    ///
    /// 返回 `true` 表示后端已成功切换到 XSynth，调用方应据此更新播放 MIDI 输出。
    pub fn check_async_init_complete(&mut self) -> bool {
        if !self.is_xsynth_initializing {
            return false;
        }

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
            SynthBackend::XSynth => {
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
            // XSynth 切换后还 fallback 才值得警告
            if matches!(self.active_backend, SynthBackend::XSynth) {
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
            SynthBackend::XSynth => {
                tracing::info!("MIDI 输入 API: XSynth 不支持输入，使用 System 后端");
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

        // 更新偏好后端
        self.preferred_backend = ui_config.preferred_backend;

        // 更新 XSynth 配置（供 create_additional_output 回退创建时使用）
        self.xsynth_soundfont_path = ui_config.soundfont_path.clone();
        self.xsynth_buffer_ms = ui_config.xsynth_buffer_ms;
        self.xsynth_threads = ui_config.xsynth_threads;
        self.xsynth_sample_rate = ui_config.xsynth_sample_rate;
        self.xsynth_fade_out_killing = ui_config.xsynth_fade_out_killing;
        self.xsynth_max_voices_per_key = ui_config.xsynth_max_voices_per_key;
        self.xsynth_global_voice_limit = ui_config.xsynth_global_voice_limit;

        // 清空 SoundFont 缓存，防止旧条目无限累积（每个 SF2 30-300MB）
        lumino_midi_io::soundfont_cache::clear_cache();

        // 关闭旧的 MIDI 输出和备用 API
        if let Some(old_output) = self.output.take() {
            drop(old_output);
        }
        self.fallback_api = None;
        self.xsynth_init_rx = None;
        self.is_xsynth_initializing = false;

        // 重新初始化
        if ui_config.preferred_backend == SynthBackend::XSynth {
            // 先快速启动 System，然后后台初始化 XSynth
            let system_result = Self::init_system_output();
            self.api = system_result.api;
            self.output = system_result.output;
            self.active_backend = system_result.backend;
            self.start_xsynth_async_init(ui_config);
        } else {
            // 初始化其他后端
            let backend_result = match ui_config.preferred_backend {
                SynthBackend::Kdmapi => Self::init_kdmapi_output(),
                SynthBackend::System => Self::init_system_output(),
                _ => Self::init_system_output(),
            };
            self.api = backend_result.api;
            self.output = backend_result.output;
            self.active_backend = backend_result.backend;
        }

        tracing::info!("MIDI 输出已重新初始化，实际后端: {:?}", self.active_backend);
    }
}

/// 处理音频动作
pub fn handle_audio_action(
    output: &mut Box<dyn lumino_midi_io::OutputConnection>,
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
