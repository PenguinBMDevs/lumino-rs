use lumino_core::storage::config::{SynthBackend, UiConfig};
use std::path::PathBuf;

/// MIDI 设备管理器
///
/// 负责管理 MIDI API 和输出连接的生命周期
pub struct MidiManager {
    /// 保存 API 实例（用于保持 RealtimeSynth 等存活）
    api: Option<Box<dyn lumino_midi::Api>>,
    /// MIDI 输出连接
    output: Option<Box<dyn lumino_midi::OutputConnection>>,
    /// 实际启用的合成器后端
    active_backend: SynthBackend,
    /// 是否需要重新初始化
    needs_reinit: bool,
}

impl Default for MidiManager {
    fn default() -> Self {
        Self {
            api: None,
            output: None,
            active_backend: SynthBackend::System,
            needs_reinit: false,
        }
    }
}

impl MidiManager {
    /// 从配置初始化 MIDI 管理器
    pub fn from_config(ui_config: &UiConfig) -> Self {
        let (api, output, backend) = Self::init_midi_output(ui_config);
        Self {
            api,
            output,
            active_backend: backend.unwrap_or(SynthBackend::System),
            needs_reinit: false,
        }
    }

    /// 获取当前激活的后端
    pub fn active_backend(&self) -> SynthBackend {
        self.active_backend
    }

    /// 获取 MIDI 输出连接的可变引用
    pub fn output_mut(&mut self) -> Option<&mut Box<dyn lumino_midi::OutputConnection>> {
        self.output.as_mut()
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

        // 关闭旧的 MIDI 输出
        if let Some(old_output) = self.output.take() {
            drop(old_output);
        }
        // 注意：api 会在新 API 创建时被替换

        // 重新初始化
        let (new_api, new_output, new_backend) = Self::init_midi_output(ui_config);
        self.api = new_api;
        self.output = new_output;
        self.active_backend = new_backend.unwrap_or(SynthBackend::System);

        tracing::info!("MIDI 输出已重新初始化，实际后端: {:?}", self.active_backend);
    }

    /// 初始化 MIDI 输出
    fn init_midi_output(
        ui_config: &UiConfig,
    ) -> (
        Option<Box<dyn lumino_midi::Api>>,
        Option<Box<dyn lumino_midi::OutputConnection>>,
        Option<SynthBackend>,
    ) {
        use lumino_midi::ApiKind;

        // 优先级顺序：XSynth -> Kdmapi -> System
        let chosen_backend = Self::select_backend(ui_config);

        // 如果都失败，使用 System
        let (api_kind, actual_backend) =
            chosen_backend.unwrap_or((ApiKind::System, SynthBackend::System));

        // 初始化 MIDI API
        let api = match lumino_midi::new_api(&api_kind) {
            Ok(api) => api,
            Err(e) => {
                tracing::warn!("初始化 MIDI API 失败: {:?}", e);
                return (None, None, Some(actual_backend));
            }
        };

        if let Some(version) = api.version() {
            tracing::info!("MIDI API 版本: {}", version);
        }

        // 获取第一个可用的输出设备
        let outputs = match api.outputs() {
            Ok(outputs) => outputs,
            Err(e) => {
                tracing::warn!("获取 MIDI 输出设备失败: {:?}", e);
                return (Some(api), None, Some(actual_backend));
            }
        };

        if let Some(output) = outputs.first() {
            tracing::info!("使用 MIDI 输出设备: {}", output.name);
            match api.open_output(output.id) {
                Ok(conn) => {
                    tracing::info!("MIDI 输出连接已打开");
                    (Some(api), Some(conn), Some(actual_backend))
                }
                Err(e) => {
                    tracing::warn!("打开 MIDI 输出连接失败: {:?}", e);
                    (Some(api), None, Some(actual_backend))
                }
            }
        } else {
            tracing::warn!("未找到可用的 MIDI 输出设备");
            (Some(api), None, Some(actual_backend))
        }
    }

    /// 选择 MIDI 后端
    fn select_backend(ui_config: &UiConfig) -> Option<(lumino_midi::ApiKind, SynthBackend)> {
        // 1. 尝试 XSynth
        if let SynthBackend::XSynth = ui_config.preferred_backend
            && let Some(backend) = Self::try_xsynth(ui_config)
        {
            return Some(backend);
        }

        // 2. 尝试 Kdmapi
        if let Some(backend) = Self::try_kdmapi(ui_config) {
            return Some(backend);
        }

        None
    }

    /// 尝试初始化 XSynth
    fn try_xsynth(ui_config: &UiConfig) -> Option<(lumino_midi::ApiKind, SynthBackend)> {
        use lumino_midi::ApiKind;

        if ui_config.soundfont_path.is_empty() {
            tracing::warn!("XSynth 音色库路径未设置");
            return None;
        }

        let path = PathBuf::from(&ui_config.soundfont_path);
        if !path.exists() {
            tracing::warn!("XSynth 音色库文件不存在: {:?}", path);
            return None;
        }

        Some((
            ApiKind::XSynth {
                soundfont_path: path,
            },
            SynthBackend::XSynth,
        ))
    }

    /// 尝试初始化 Kdmapi
    fn try_kdmapi(ui_config: &UiConfig) -> Option<(lumino_midi::ApiKind, SynthBackend)> {
        use lumino_midi::ApiKind;

        let kdmapi_path = PathBuf::from("C:\\Windows\\System32\\OmniMIDI\\OmniMIDI.dll");
        if !kdmapi_path.exists() {
            tracing::warn!("未找到 OmniMIDI");
            return None;
        }

        if let SynthBackend::XSynth = ui_config.preferred_backend {
            tracing::info!("XSynth 不可用，回退到 KDMAPI");
        }

        Some((ApiKind::Kdmapi { path: kdmapi_path }, SynthBackend::Kdmapi))
    }
}

/// 处理音频动作
pub fn handle_audio_action(
    output: &mut Box<dyn lumino_midi::OutputConnection>,
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
