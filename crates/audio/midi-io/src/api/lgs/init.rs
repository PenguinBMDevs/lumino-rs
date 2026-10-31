//! LGS (GPU) 后端初始化：构造、GPU 引擎构建、音频流启动。
//!
//! 自 `lgs.rs` 拆出（文件行数守卫 <400）。含 #149 返工诊断的分段打点：
//! GPU 上下文 / 音色库 / 设备解析 / 流启动各自记录耗时，便于定位
//! "某个调用不返回"（原生 wgpu/cpal 路径可能在驱动层阻塞）。

use super::*;

impl Lgs {
    /// 由 LGS 选项构造 GPU `SynthConfig`（纯函数，便于单测断言 REND-016 复音上限透传）。
    pub(super) fn synth_config_from(options: &LgsOptions) -> SynthConfig {
        SynthConfig {
            sample_rate: options.sample_rate,
            block_size: options.block_size,
            max_voices: options.max_voices,
            max_voices_per_key: options.max_voices_per_key,
            midi_channels: channels_for_max_port_clamped(options.midi_max_port) as usize,
            interpolation: if options.use_sinc {
                InterpolationMode::Point64Sinc
            } else {
                InterpolationMode::Linear
            },
            ..SynthConfig::default()
        }
    }

    /// 构建 GPU 合成引擎（含音色库加载；不涉及音频流）。
    ///
    /// `new` 与 `set_midi_port_layout` 重建共用；端口布局决定
    /// `SynthConfig.midi_channels`（对齐 XSynth `rt_format`）。
    ///
    /// 分段打点（#149 返工诊断）：`GpuSynth::new` 会走 wgpu 实例/适配器/设备
    /// 创建（原生 Vulkan/DX12 路径），是"调用不返回"的高危点；日志可定位卡点。
    pub(super) fn build_synth(
        soundfont_path: &Path,
        options: &LgsOptions,
    ) -> Result<GpuSynth, Error> {
        let config = Self::synth_config_from(options);

        tracing::info!("LGS (GPU): 创建 GPU 设备上下文（wgpu adapter/device）...");
        let t_gpu = std::time::Instant::now();
        let mut synth = GpuSynth::new(config)
            .map_err(|e| Error::InitFailed(trf!("LGS (GPU) 初始化失败: {e}", e = e)))?;
        tracing::info!(
            "LGS (GPU): GPU 设备上下文就绪（耗时 {}ms），开始加载音色库: {:?}",
            t_gpu.elapsed().as_millis(),
            soundfont_path
        );

        let t_sf = std::time::Instant::now();
        synth
            .load_soundfont(soundfont_path, 0, 0)
            .map_err(|e| Error::InitFailed(trf!("LGS (GPU) 音色库加载失败: {e}", e = e)))?;
        tracing::info!(
            "LGS (GPU): 音色库加载完成（耗时 {}ms）",
            t_sf.elapsed().as_millis()
        );
        Ok(synth)
    }

    /// 使用指定音色库路径与选项创建 LGS 后端
    pub fn new(soundfont_path: &Path, options: &LgsOptions) -> Result<Self, Error> {
        tracing::info!("LGS (GPU): 初始化，音色库路径: {:?}", soundfont_path);

        if !soundfont_path.exists() {
            return Err(Error::InitFailed(format!(
                "Soundfont file not found: {:?}",
                soundfont_path
            )));
        }

        let synth = Self::build_synth(soundfont_path, options)?;

        // 解析音频播放输出设备：指定设备有效则对其打开流，
        // 否则回退到系统默认输出设备。
        // 分段打点（#149 返工诊断）：cpal 设备枚举/流协商同样可能在驱动层阻塞。
        tracing::info!(
            "LGS (GPU): 解析音频输出设备（指定: {}）...",
            options
                .audio_output_device
                .as_deref()
                .unwrap_or("<系统默认>")
        );
        let t_dev = std::time::Instant::now();
        let device = crate::audio_devices::resolve_audio_output_device(
            options.audio_output_device.as_deref(),
        );
        tracing::info!(
            "LGS (GPU): 音频输出设备已解析（耗时 {}ms）",
            t_dev.elapsed().as_millis()
        );

        tracing::info!("LGS (GPU): 启动音频流（cpal 协商 + 流启动）...");
        let t_stream = std::time::Instant::now();
        let playback = AudioPlayback::start(synth, device, options.soft_nps_gate)
            .map_err(|e| Error::InitFailed(trf!("LGS (GPU) 音频流启动失败: {e}", e = e)))?;
        tracing::info!(
            "LGS (GPU): 音频流已启动（耗时 {}ms）",
            t_stream.elapsed().as_millis()
        );
        let event_tx = Arc::new(SharedEventTx::new(playback.event_sender()));
        let control_tx = Arc::new(Mutex::new(playback.control_sender()));
        let velocity_filter = Arc::new(AtomicU8::new(options.velocity_filter_threshold));
        // 统计读取器（克隆句柄，读取无锁原子）
        let stats = playback.stats();

        let version = format!("lumino-gpu-synth {}", lumino_gpu_synth::VERSION);
        tracing::info!(
            "LGS (GPU): 初始化完成（midi_max_port={}，midi_channels={}，max_voices={}，sample_rate={}，block_size={}）",
            options.midi_max_port,
            channels_for_max_port_clamped(options.midi_max_port),
            options.max_voices,
            options.sample_rate,
            options.block_size
        );

        Ok(Self {
            _playback: Arc::new(Mutex::new(playback)),
            event_tx,
            control_tx,
            midi_max_port: options.midi_max_port,
            velocity_filter,
            soundfont_path: soundfont_path.to_path_buf(),
            options: options.clone(),
            stats,
            version,
        })
    }
}
