use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cpal::traits::DeviceTrait;
use xsynth_core::{
    AudioStreamParams, ChannelCount,
    channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent},
    soundfont::SoundfontBase,
};

use crate::realtime::{
    ChannelMixHandle, RealtimeEventSender, RealtimeSynth, StreamRestartError, SynthEvent,
    ThreadCount as LuminoThreadCount, XSynthRealtimeConfig,
};

use super::xsynth_output::XSynthOutputConn;
use crate::soundfont_cache;
use crate::{
    Api, Error, InputConnection, InputInfo, MidiInputCallback, OutputConnection, OutputInfo,
};

/// XSynth 运行时统计信息
#[derive(Debug, Clone, Copy, Default)]
pub struct XSynthStats {
    /// 当前活跃 voice 数量
    pub voice_count: u64,
    /// 渲染器平均负载 (0.0 - 1.0)
    pub average_renderer_load: f64,
    /// 缓冲区样本数
    pub buffer_samples: i64,
}

/// XSynth 后端打开选项
#[derive(Debug, Clone)]
pub struct XSynthOptions {
    /// 缓冲区时长（毫秒）
    pub buffer_ms: f64,
    /// 每个键的最大并发 voice 数（None / 0 = 不限制；上限 128）。
    /// 对应 `ChannelConfigEvent::SetLayerCount`，调高减少偷声但增加渲染负载。
    pub max_voices_per_key: Option<usize>,
    /// 采样率
    pub sample_rate: u32,
    /// 每通道活跃声部上限（None = 不限，Some(0) 同样视为不限）。
    ///
    /// 注意：上游默认按**每通道**治理；多数黑乐谱跨多通道，实际总预算会被
    /// 通道数放大。实时路径固定 `None`，统一使用 `global_max_voices`（跨通道全局上限）。
    pub max_voices_per_channel: Option<usize>,
    /// 跨通道全局声部上限（硬上限/量程；None = 自动，引擎默认 10000）。
    ///
    /// 由渲染管线统一调度：运行目标 = `voice_target_ratio × 硬上限`，
    /// 超限时向"声部最多的通道"下发抢占命令，保证活跃声部总数有上界，
    /// 且新音符不被丢弃。
    pub global_max_voices: Option<usize>,
    /// 复音软目标比例：运行目标 = 比例 × `global_max_voices`（负载反馈只会更低）。
    ///
    /// 默认 `1 - 1/e ≈ 0.632`，留出约 37% 暂态余量，避免过载后的无休止正反馈。
    pub voice_target_ratio: f64,
    /// 过载保命闸（软 NPS 闸）：仅在重度过载时临时限速，默认关闭。
    ///
    /// 关闭时引擎不存在任何 NoteOn 丢弃路径。
    pub soft_nps_gate: bool,
    /// 音频播放输出设备（CPAL 音频设备名；None = 使用系统默认输出设备）
    pub audio_output_device: Option<String>,
}

/// 探测进程可用逻辑核数（至少为 1）。
fn available_parallelism() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// 线程池逃生口环境变量（**仅供 A/B 复测**，不对外暴露为设置项）。
const THREAD_POOL_ENV: &str = "XSYNTH_THREAD_POOL";

/// 解析线程池逃生口（纯函数，便于单测）。
///
/// - 未设置 / `0` / `none` / `off` / `false` / 非法值 → `ThreadCount::None`（默认策略）；
/// - `manual` / `on` / `true` → `Manual(逻辑核数)`（复现历史行为，用于在高核机器上对比）；
/// - 正整数 → `Manual(n)`（指定池线程数）。
fn parse_thread_pool_override(value: Option<&str>, logical_cores: usize) -> LuminoThreadCount {
    match value.map(str::trim) {
        None | Some("") | Some("0") | Some("none") | Some("off") | Some("false") => {
            LuminoThreadCount::None
        }
        Some("manual") | Some("on") | Some("true") => {
            LuminoThreadCount::Manual(logical_cores.max(1))
        }
        Some(raw) => raw
            .parse::<usize>()
            .ok()
            .filter(|threads| *threads > 0)
            .map(LuminoThreadCount::Manual)
            .unwrap_or(LuminoThreadCount::None),
    }
}

/// 线程策略：**始终不使用通道内并行池**（`ThreadCount::None`）。
///
/// 每通道一个独立渲染线程（16 通道 = 16 线程），通道之间互不耦合。
///
/// 为什么不按核数切换到通道内 rayon 池（历史 `>16 逻辑核 → Manual(核数)` 分支）：
///
/// 1. 该池是**全部 16 个通道共享**的（`prepare_channels` 里 `Arc<ThreadPool>` 被克隆给每个
///    通道），因此并不提供"每通道更多并行"，只是把 16 个通道串行排进同一个池 —— 队头阻塞。
///    既有实测（16 通道并发 × 每通道 ~512 voices、100ms 块、12/6/4/2 逻辑核四档）：
///    共享池最坏单块 0.96~4.6s，无池最坏单块 0.14~0.57s，无池全面不劣。
/// 2. `ThreadCount::Manual(_)` 会让 xsynth 侧 `set_batch_render_available(false)`
///    （见 `RealtimeSynth::open`），**连带关闭 B1 跨 voice 批渲染**——那是 fork 的核心优化。
///    于是 >16 核机器同时失去批渲染、又背上队头阻塞，是双重劣势。
/// 3. 高核机器上剩余的核并非没有用处：缓冲渲染线程、音频回调、播放线程与 UI 都在抢核，
///    `>16` 分支把"额外核可用"直接等同于"通道内并行更划算"，缺少实测支撑。
///
/// 因此本函数不再按核数分支：**任何核数都走无池 + 批渲染**。
/// 需要在真实高核机器上复测尾延迟时，用 `XSYNTH_THREAD_POOL=manual` 一键切回池化做 A/B。
fn forced_thread_count(logical_cores: usize) -> LuminoThreadCount {
    let raw = std::env::var(THREAD_POOL_ENV).ok();
    let mode = parse_thread_pool_override(raw.as_deref(), logical_cores);
    if !matches!(mode, LuminoThreadCount::None) {
        tracing::warn!(
            "XSynth: {THREAD_POOL_ENV}={:?} 覆盖线程策略 → {mode:?}（仅供 A/B 复测，会关闭批渲染）",
            raw.unwrap_or_default()
        );
    }
    mode
}

/// 按当前机器强制解析线程模式（供 `init_synth` 使用）。
fn machine_thread_count() -> LuminoThreadCount {
    let logical = available_parallelism();
    tracing::debug!("XSynth: 固定线程策略（{logical} 逻辑核，无通道内并行池、启用批渲染）");
    forced_thread_count(logical)
}

/// 渲染块时长（ms）：MIDI 事件按渲染块边界批量应用，块越小音符落点量化误差越小。
const RENDER_WINDOW_MS: f64 = 10.0;

/// 缓冲目标地板（ms）。
///
/// 用户的"缓冲区"设置被用作**总缓冲目标**（与渲染块解耦），但后端强制 ≥ 该值：
/// 渲染尖峰与系统调度抖动需要深缓冲兜底，低于此值在重载下会出现欠载/爆音。
///
/// 与 UI 的不一致必须显式告警而非静默抬升：UI 滑块量程 5–100ms、
/// 默认值 30ms（`default_synth_buffer`），因此**默认配置本身就低于地板**，
/// 用户拉到任何值（含默认）实际都按 100ms 运行。
const MIN_CUSHION_MS: f64 = 100.0;

/// 归一化每键最大同音数：`None` / `0` = 不限制；其余夹紧到 1..=128。
///
/// 注意绝不能把 0 直接传给 `SetLayerCount(Some(0))`：xsynth 会立即偷声，
/// 导致该键所有新音符无声（0 按"不限制"处理是产品约定）。
fn normalize_max_voices_per_key(value: Option<usize>) -> Option<usize> {
    match value {
        None | Some(0) => None,
        Some(v) => Some(v.clamp(1, 128)),
    }
}

/// XSynth 软件合成后端，基于 realtime 合成管线提供实时 MIDI 播放
pub struct XSynth {
    synth: RealtimeSynth,
    /// 共享事件发送器（全量重建时替换，所有已创建的输出连接自动跟随）
    sender_shared: Arc<Mutex<RealtimeEventSender>>,
    /// 混音参数共享句柄（重建稳定：外层 `Arc` 指针不变，重建时替换内层 `Vec`）。
    /// 所有已创建的 `XSynthOutputConn` 通过它设置每通道增益/声像，
    /// 与 `sender_shared` 同生命周期语义。
    mixer_shared: ChannelMixHandle,
    /// 主输出实时响度峰值共享句柄（重建稳定：`clone_master_peak` 的 `Arc` 不变，
    /// 重建时跟随新管线），供输出连接读取主输出电平。
    master_peak_shared: Arc<AtomicU32>,
    /// 音色库路径（重建管线时重用）
    soundfont_path: PathBuf,
    /// 打开选项（重建管线时重用）
    options: Option<XSynthOptions>,
    version: String,
}

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

    /// 获取运行时统计信息
    pub fn stats(&self) -> XSynthStats {
        let stats = self.synth.get_stats();
        XSynthStats {
            voice_count: stats.voice_count(),
            average_renderer_load: stats.buffer().average_renderer_load(),
            buffer_samples: stats.buffer().last_samples_after_read(),
        }
    }

    /// 检查音频流是否因设备移除等不可用，需要恢复。
    ///
    /// 底层（xsynth-realtime）在音频设备被拔出/更换时自动尝试重定向到
    /// 系统默认输出设备；仅当自愈失败（如新设备参数与管线不一致）时才返回 `true`。
    pub fn poll_stream_recovery_needed(&self) -> bool {
        self.synth.poll_recovery_error().is_some()
    }

    /// 恢复音频流：优先直接重定向到系统默认输出设备（合成管线不变），
    /// 重定向不可行（设备参数变化）时全量重建合成管线。
    pub fn recover_stream(&mut self) -> Result<(), String> {
        match self.synth.restart_stream() {
            Ok(()) => {
                tracing::info!("XSynth: 音频流已重定向到默认输出设备（合成管线保持不变）");
                Ok(())
            }
            Err(StreamRestartError::ConfigChanged(msg)) => {
                tracing::warn!("XSynth: 设备参数已改变 ({msg})，重建合成管线");
                self.rebuild()
            }
            Err(e) => {
                tracing::warn!("XSynth: 音频流重定向失败 ({e})，重建合成管线");
                self.rebuild()
            }
        }
    }

    /// 全量重建合成管线（使用当前系统默认输出设备）。
    ///
    /// 重建后替换共享事件发送器，所有已创建的 `XSynthOutputConn` 自动跟随新管线；
    /// 无需上层重建输出连接。
    fn rebuild(&mut self) -> Result<(), String> {
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

impl Api for XSynth {
    fn version(&self) -> Option<String> {
        Some(self.version.clone())
    }

    fn inputs(&self) -> Result<Vec<InputInfo>, Error> {
        Ok(Vec::new())
    }

    fn outputs(&self) -> Result<Vec<OutputInfo>, Error> {
        Ok(vec![OutputInfo {
            id: 0,
            name: "XSynth".to_string(),
        }])
    }

    fn open_output(&self, id: u32) -> Result<Box<dyn OutputConnection>, Error> {
        if id != 0 {
            return Err(Error::DeviceNotFound(id));
        }
        Ok(Box::new(XSynthOutputConn {
            sender: Arc::clone(&self.sender_shared),
            mixer: Arc::clone(&self.mixer_shared),
            master: Arc::clone(&self.master_peak_shared),
        }))
    }

    fn open_input(
        &self,
        _id: u32,
        _callback: MidiInputCallback,
    ) -> Result<Box<dyn InputConnection>, Error> {
        Err(Error::InitFailed(
            "XSynth does not support MIDI input".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_policy_never_uses_channel_pool_by_default() {
        // 关键回归：任何核数都必须走「无池 + 批渲染」，
        // 不得再按 >16 逻辑核切到「共享池 + 关批渲染」的历史分支。
        for cores in [1usize, 2, 4, 6, 12, 16, 17, 24, 32, 64] {
            assert_eq!(
                parse_thread_pool_override(None, cores),
                LuminoThreadCount::None,
                "{cores} 逻辑核默认必须无通道内池"
            );
        }
        assert!(available_parallelism() >= 1, "逻辑核数应至少为 1");
        // 未设置逃生口时，真实解析路径同样恒为无池。
        if std::env::var(THREAD_POOL_ENV).is_err() {
            assert_eq!(forced_thread_count(32), LuminoThreadCount::None);
        }
    }

    #[test]
    fn thread_pool_override_parses_only_whitelisted_values() {
        // 只有显式白名单才允许切回池化（A/B 复测用）；其余一律保持默认无池。
        for value in [
            None,
            Some(""),
            Some("   "),
            Some("0"),
            Some("none"),
            Some("off"),
            Some("false"),
            Some("bogus"),
            Some("-1"),
        ] {
            assert_eq!(
                parse_thread_pool_override(value, 24),
                LuminoThreadCount::None,
                "{value:?} 应保持默认无池"
            );
        }
        for value in [Some("manual"), Some("on"), Some("true"), Some(" manual ")] {
            assert_eq!(
                parse_thread_pool_override(value, 24),
                LuminoThreadCount::Manual(24),
                "{value:?} 应强制池化"
            );
        }
        assert_eq!(
            parse_thread_pool_override(Some("8"), 24),
            LuminoThreadCount::Manual(8)
        );
        // 异常环境（逻辑核数 0）也不得构造出合法的 Manual(0)。
        assert_eq!(
            parse_thread_pool_override(Some("manual"), 0),
            LuminoThreadCount::Manual(1)
        );
    }

    #[test]
    fn normalize_max_voices_per_key_bounds() {
        assert_eq!(normalize_max_voices_per_key(None), None);
        assert_eq!(
            normalize_max_voices_per_key(Some(0)),
            None,
            "0 按不限制处理，绝不能下发 Some(0)"
        );
        assert_eq!(normalize_max_voices_per_key(Some(1)), Some(1));
        assert_eq!(normalize_max_voices_per_key(Some(16)), Some(16));
        assert_eq!(
            normalize_max_voices_per_key(Some(200)),
            Some(128),
            "上限应夹紧到 128"
        );
    }
}
