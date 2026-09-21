use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cpal::traits::DeviceTrait;
use xsynth_core::{
    AudioStreamParams, ChannelCount,
    channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent},
    soundfont::SoundfontBase,
};

use crate::Error;
use crate::realtime::{
    ChannelMixHandle, RealtimeEventSender, RealtimeSynth, SynthEvent, XSynthRealtimeConfig,
};
use crate::soundfont_cache;

use super::threads::machine_thread_count;
use super::{
    MIN_CUSHION_MS, RENDER_WINDOW_MS, XSynth, XSynthOptions, normalize_max_voices_per_key,
};

impl XSynth {
    /// 使用指定音色库路径创建 XSynth 后端
    pub fn new(soundfont_path: &Path, options: Option<XSynthOptions>) -> Result<Self, Error> {
        tracing::info!("XSynth: 初始化，音色库路径: {:?}", soundfont_path);

        // 检查音色库文件是否存在
        if !soundfont_path.exists() {
            return Err(Error::InitFailed(format!(
                "Soundfont file not found: {:?}",
                soundfont_path
            )));
        }

        let (synth, sender) = Self::init_synth(soundfont_path, options.as_ref())?;
        let sender_shared = Arc::new(Mutex::new(sender));
        let mixer_shared = ChannelMixHandle::new(Mutex::new(synth.clone_channel_mix()));
        let master_peak_shared = synth.clone_master_peak();

        let version = "xsynth-realtime 0.4.0 (lumino-realtime)".to_string();
        tracing::info!("XSynth: 初始化完成");

        Ok(Self {
            synth,
            sender_shared,
            mixer_shared,
            master_peak_shared,
            soundfont_path: soundfont_path.to_path_buf(),
            options,
            version,
        })
    }

    /// 初始化合成管线：预加载音色库 → 打开音频流 → 配置音色库事件。
    ///
    /// 被 `new` 与 `rebuild`（设备参数变化后全量重建）复用。
    fn init_synth(
        soundfont_path: &Path,
        options: Option<&XSynthOptions>,
    ) -> Result<(RealtimeSynth, RealtimeEventSender), Error> {
        // 采样率对齐原则：配置中的 `xsynth_sample_rate` 仅作提示，
        // 真正决定音高的是设备实际采样率。`RealtimeSynth`（xsynth-realtime）
        // 总是以 cpal 设备原生采样率渲染，因此音色库必须以同一采样率预处理，
        // 否则 SampleSoundfont 内部重采样/包络时间错配会产生音高偏移（跑调）。
        //
        // 加载策略（性能修复）：先打开音频流取得设备原生采样率，再「只」按实际采样率
        // 加载一次。
        //  - 缓存键为 (path, sample_rate)，若先按请求采样率(默认 44100)占位预加载、
        //    再按实际采样率(多为 48000)加载，采样率不一致时会触发一次完整的
        //    SampleSoundfont 全量构建冗余，并在全局缓存留下一份永不被使用的数十~数百 MB 条目；
        //  - 占位预加载的 soundfont 从未经 SetSoundfonts 下发合成器，对「流启动防 underrun」
        //    零作用（流启动后本就静音，直到下方 SetSoundfonts 到达，与 Core 后端空 Group 一致）。
        // 故仅加载实际采样率一份，零冗余。
        // 线程策略：不对外提供控制，按机器核数强制解析（逻辑核 > 16 才启用通道内并行池）。
        // `UiConfig::xsynth_threads` 已废弃、不再读取，保留字段仅为兼容旧配置。
        //
        // NPS 限流：fork 默认 10_000，超过阈值即静默丢弃 NoteOn（按力度加权，
        // 黑 MIDI 密集段实测可丢 99%+），对黑 MIDI 编辑器属功能性缺陷。
        // 显式关闭（0 = 不限流）；发声规模仍由每键 layers 上限约束，不会无限膨胀。
        // 如需恢复保护，把 0 改为具体阈值即可（按通道估算 NPS，非全局限流）。
        // 实时后端不启用 fade_out_killing（引擎默认 false，被杀 voice 立即出队）：
        // 该选项会让被杀 voice 滞留到本次渲染结束才清除，黑 MIDI 高密度事件积压时
        // 每个 NoteOn 的全 buffer 扫描退化为 O(n²)，实测渲染负载 13~28 倍实时且
        // 长时间无法恢复（死亡螺旋），故产品层直接移除该开关。
        let mut rt_config = XSynthRealtimeConfig {
            multithreading: machine_thread_count(),
            max_nps: 0,
            ..Default::default()
        };

        if let Some(opt) = options {
            // 渲染块固定 10ms：MIDI 事件按渲染块边界批量应用，块越小音符落点
            // 量化误差越小（节奏更准）；用户的"缓冲区"设置改为总缓冲目标，
            // 与块大小解耦，用于吸收渲染尖峰与系统调度抖动。
            rt_config.render_window_ms = RENDER_WINDOW_MS;
            // 缓冲目标地板：低于地板一律按地板运行，并显式告警（不再静默抬升）。
            let requested_cushion_ms = opt.buffer_ms;
            let cushion_ms = requested_cushion_ms.max(MIN_CUSHION_MS);
            if requested_cushion_ms < MIN_CUSHION_MS {
                tracing::warn!(
                    "XSynth: 缓冲区设置 {:.1}ms 低于后端地板 {:.0}ms，实际按 {:.0}ms 运行；\
                     UI 滑块量程/默认值需与后端地板对齐（当前不一致会让该设置看起来无效）",
                    requested_cushion_ms,
                    MIN_CUSHION_MS,
                    cushion_ms
                );
            }
            rt_config.cushion_ms = cushion_ms;

            // 每通道上限关闭（None），改用跨通道全局上限。
            rt_config.channel_init_options.max_voices = opt.max_voices_per_channel;
            // 跨通道全局声部上限：由渲染管线统一治理（实时主路径）。
            rt_config.global_max_voices = opt.global_max_voices;
            // 复音软目标比例：运行目标 = 比例 × 硬上限，负载反馈只会更低。
            rt_config.voice_target_ratio = opt.voice_target_ratio;
            // 过载保命闸：默认关闭（关闭时无任何 NoteOn 丢弃路径）。
            rt_config.soft_nps_gate = opt.soft_nps_gate;
        }

        // 解析音频播放输出设备：指定设备有效则直接对其打开流，
        // 否则回退到系统默认输出设备（设备已移除 / 改名时优雅降级）。
        let target_device = crate::audio_devices::resolve_audio_output_device(
            options.and_then(|o| o.audio_output_device.as_deref()),
        );
        let synth = match target_device {
            Some(device) => {
                let stream_config = device
                    .default_output_config()
                    .map_err(|e| Error::InitFailed(format!("音频设备默认配置失败: {}", e)))?;
                RealtimeSynth::open(rt_config, &device, stream_config)
                    .map_err(|e| Error::InitFailed(format!("xsynth-realtime: {}", e)))?
            }
            None => RealtimeSynth::open_with_default_output(rt_config)
                .map_err(|e| Error::InitFailed(format!("xsynth-realtime: {}", e)))?,
        };

        // 设备实际采样率（cpal 决定，可能与请求的不同）
        let actual_sample_rate = synth.stream_params().sample_rate;

        // 仅按设备实际采样率加载一次正式音色库（无占位预加载）。SampleSoundfont::new()
        // 的预处理（采样率转换、包络时间等）与 AudioStreamParams.sample_rate 强相关，
        // 混用会导致音高/速度错误（跑调），故无条件以 actual_sample_rate 加载。
        let actual_params = AudioStreamParams::new(actual_sample_rate, ChannelCount::Stereo);
        let load_start = Instant::now();
        let soundfont = soundfont_cache::load_soundfont_cached(soundfont_path, actual_params)
            .map_err(Error::InitFailed)?;
        tracing::info!(
            "XSynth: 音色库已按设备实际采样率 {}Hz 加载，耗时: {:.2} 秒",
            actual_sample_rate,
            load_start.elapsed().as_secs_f64()
        );

        // 获取 sender — 在 open 后立即配置通道，确保音色库在 callback 首次触发前就位
        let mut sender = synth.get_sender_ref().clone();

        // 配置音色库
        let soundfonts: Vec<Arc<dyn SoundfontBase>> = vec![soundfont];
        sender.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
            ChannelConfigEvent::SetSoundfonts(soundfonts),
        )));

        // 每键最大同音数（layers）：把用户设置真正下发。此前该设置从未接线，
        // 引擎始终用默认 Some(4)；None / 0 = 不限制，其余夹紧 1..=128。
        sender.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
            ChannelConfigEvent::SetLayerCount(normalize_max_voices_per_key(
                options.and_then(|o| o.max_voices_per_key),
            )),
        )));

        // 重置所有通道，确保音色库生效
        sender.send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
            ChannelAudioEvent::AllNotesKilled,
        )));

        sender.send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
            ChannelAudioEvent::ResetControl,
        )));

        Ok((synth, sender))
    }

    /// 全量重建合成管线（使用当前系统默认输出设备）。
    ///
    /// 重建后替换共享事件发送器，所有已创建的 `XSynthOutputConn` 自动跟随新管线；
    /// 无需上层重建输出连接。
    pub(super) fn rebuild(&mut self) -> Result<(), String> {
        let (synth, sender) = Self::init_synth(&self.soundfont_path, self.options.as_ref())
            .map_err(|e| format!("重建合成管线失败: {e}"))?;

        // 替换合成器（旧实例 drop：发送 Shutdown 并 join 全部线程）
        self.synth = synth;
        // 替换共享发送器：已创建的输出连接通过 Arc 读取，自动指向新管线
        *self.sender_shared.lock().unwrap_or_else(|e| e.into_inner()) = sender;
        // 替换混音句柄：已创建的输出连接通过外层 Arc 读取，自动指向新管线
        *self.mixer_shared.lock().unwrap_or_else(|e| e.into_inner()) =
            self.synth.clone_channel_mix();

        tracing::info!("XSynth: 合成管线已重建");
        Ok(())
    }
}
