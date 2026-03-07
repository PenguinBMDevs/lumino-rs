use lumino_core::storage::config::{SynthBackend, UiConfig};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

/// XSynth 异步初始化结果
enum XSynthInitResult {
    Success {
        api: Box<dyn lumino_midi::Api>,
        output: Box<dyn lumino_midi::OutputConnection>,
    },
    Failed(String),
}

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
    /// 配置中偏好的后端（用于异步初始化后知道应该切换到哪个后端）
    preferred_backend: SynthBackend,
    /// XSynth 异步初始化接收器
    xsynth_init_rx: Option<Receiver<XSynthInitResult>>,
    /// 是否正在异步初始化 XSynth
    is_xsynth_initializing: bool,
}

impl Default for MidiManager {
    fn default() -> Self {
        Self {
            api: None,
            output: None,
            active_backend: SynthBackend::System,
            needs_reinit: false,
            preferred_backend: SynthBackend::System,
            xsynth_init_rx: None,
            is_xsynth_initializing: false,
        }
    }
}

impl MidiManager {
    /// 从配置初始化 MIDI 管理器
    /// 
    /// 如果配置使用 XSynth，会先使用 System 快速启动，然后在后台初始化 XSynth
    pub fn from_config(ui_config: &UiConfig) -> Self {
        let preferred = ui_config.preferred_backend;
        
        // 首先快速启动 System 后端（不阻塞 UI）
        let (api, output, backend) = Self::init_system_output();
        
        let mut manager = Self {
            api,
            output,
            active_backend: backend,
            needs_reinit: false,
            preferred_backend: preferred,
            xsynth_init_rx: None,
            is_xsynth_initializing: false,
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
        std::thread::spawn(move || {
            tracing::info!("XSynth: 后台线程开始初始化");
            
            let result = Self::init_xsynth_blocking(&path);
            
            match &result {
                Ok(_) => tracing::info!("XSynth: 后台初始化成功"),
                Err(e) => tracing::warn!("XSynth: 后台初始化失败: {}", e),
            }
            
            let init_result = match result {
                Ok((api, output)) => XSynthInitResult::Success { api, output },
                Err(e) => XSynthInitResult::Failed(e),
            };
            
            let _ = tx.send(init_result);
        });
    }
    
    /// 阻塞式初始化 XSynth（用于后台线程）
    fn init_xsynth_blocking(
        soundfont_path: &PathBuf,
    ) -> Result<(Box<dyn lumino_midi::Api>, Box<dyn lumino_midi::OutputConnection>), String> {
        use lumino_midi::ApiKind;
        
        let api_kind = ApiKind::XSynth {
            soundfont_path: soundfont_path.clone(),
        };
        
        let api = lumino_midi::new_api(&api_kind)
            .map_err(|e| format!("初始化 MIDI API 失败: {:?}", e))?;
        
        let outputs = api.outputs()
            .map_err(|e| format!("获取输出设备失败: {:?}", e))?;
        
        let output = outputs.first()
            .ok_or("未找到可用的 MIDI 输出设备")?;
        
        let conn = api.open_output(output.id)
            .map_err(|e| format!("打开输出连接失败: {:?}", e))?;
        
        Ok((api, conn))
    }
    
    /// 快速初始化 System 后端（不阻塞）
    fn init_system_output() -> (
        Option<Box<dyn lumino_midi::Api>>,
        Option<Box<dyn lumino_midi::OutputConnection>>,
        SynthBackend,
    ) {
        use lumino_midi::ApiKind;
        
        tracing::info!("MIDI: 快速启动 System 后端");
        
        match lumino_midi::new_api(&ApiKind::System) {
            Ok(api) => {
                if let Ok(outputs) = api.outputs() {
                    if let Some(output) = outputs.first() {
                        if let Ok(conn) = api.open_output(output.id) {
                            tracing::info!("MIDI: System 后端已就绪");
                            return (Some(api), Some(conn), SynthBackend::System);
                        }
                    }
                }
                (Some(api), None, SynthBackend::System)
            }
            Err(e) => {
                tracing::warn!("MIDI: System 后端启动失败: {:?}", e);
                (None, None, SynthBackend::System)
            }
        }
    }
    
    /// 检查异步初始化是否完成，如果完成则切换到 XSynth
    pub fn check_async_init_complete(&mut self) {
        if !self.is_xsynth_initializing {
            return;
        }
        
        let rx = match &self.xsynth_init_rx {
            Some(rx) => rx,
            None => return,
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
            }
            Ok(XSynthInitResult::Failed(e)) => {
                tracing::warn!("XSynth: 异步初始化失败: {}", e);
                self.is_xsynth_initializing = false;
                self.xsynth_init_rx = None;
                // 保持在当前后端（System）
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // 还在初始化中，不做任何事
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("XSynth: 初始化线程异常断开");
                self.is_xsynth_initializing = false;
                self.xsynth_init_rx = None;
            }
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
        
        // 更新偏好后端
        self.preferred_backend = ui_config.preferred_backend;

        // 关闭旧的 MIDI 输出
        if let Some(old_output) = self.output.take() {
            drop(old_output);
        }
        self.xsynth_init_rx = None;
        self.is_xsynth_initializing = false;

        // 重新初始化
        if ui_config.preferred_backend == SynthBackend::XSynth {
            // 先快速启动 System，然后后台初始化 XSynth
            let (api, output, backend) = Self::init_system_output();
            self.api = api;
            self.output = output;
            self.active_backend = backend;
            self.start_xsynth_async_init(ui_config);
        } else {
            // 直接初始化其他后端
            let (api, output, backend) = Self::init_system_output();
            self.api = api;
            self.output = output;
            self.active_backend = backend;
        }

        tracing::info!("MIDI 输出已重新初始化，实际后端: {:?}", self.active_backend);
    }

    /// 尝试初始化 Kdmapi（同步）
    fn try_kdmapi(ui_config: &UiConfig) -> Option<(lumino_midi::ApiKind, SynthBackend)> {
        use lumino_midi::ApiKind;

        let kdmapi_path = PathBuf::from("C:\\Windows\\System32\\OmniMIDI\\OmniMIDI.dll");
        if !kdmapi_path.exists() {
            return None;
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
