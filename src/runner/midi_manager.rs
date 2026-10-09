use lumino_core::storage::config::{AudioEngineKind, SynthBackend, UiConfig};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, channel};

/// MIDI API 类型别名（REND-002：基础 trait 冻结，能力经 `SynthControl` 扩展）
type MidiApi = Box<dyn lumino_midi_io::SynthControl>;
/// MIDI 输出连接类型别名（REND-002：能力经 `PlaybackOutput` 扩展）
type MidiOutput = Box<dyn lumino_midi_io::PlaybackOutput>;
/// MIDI 初始化结果类型别名
type MidiInitResult = Result<(MidiApi, MidiOutput), String>;

/// 后端初始化结果
struct BackendInitResult {
    /// API 实例（用于保持合成器存活）
    api: Option<MidiApi>,
    /// MIDI 输出连接
    output: Option<MidiOutput>,
    /// 实际使用的后端类型
    backend: SynthBackend,
}

/// XSynth 异步初始化结果
enum XSynthInitResult {
    Success { api: MidiApi, output: MidiOutput },
    Failed(String),
}

/// LGS (GPU) 异步初始化结果
enum LgsInitResult {
    Success { api: MidiApi, output: MidiOutput },
    Failed(String),
}

/// 异步后端初始化的落定结果（#127）。
///
/// 失败路径同样需要调用方重连播放输出：同步兜底后端（System）的连接已在
/// 快速启动时建立，若不重连，播放引擎会停留在被释放/失效的旧连接上
/// →「换设备后永久无声」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncInitOutcome {
    /// 未在初始化或仍在进行中
    Pending,
    /// 已成功切换到目标后端
    Switched,
    /// 初始化失败/线程断开，保持当前后端
    Failed,
}

/// MIDI 设备管理器
///
/// 负责管理 MIDI API 和输出连接的生命周期
pub struct MidiManager {
    /// 保存 API 实例（用于保持 RealtimeSynth 等存活）
    api: Option<MidiApi>,
    /// 备用 API 实例（用于 create_additional_output 创建独立播放连接时保持存活）
    fallback_api: Option<MidiApi>,
    /// MIDI 输出连接
    output: Option<MidiOutput>,
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
    /// XSynth 采样率
    xsynth_sample_rate: u32,
    /// XSynth 每个键最大同音数
    xsynth_max_voices_per_key: Option<usize>,
    /// XSynth 全局最大并发 voice 数（硬上限/量程；None = 自动）
    xsynth_global_voice_limit: Option<usize>,
    /// XSynth 复音软目标比例（运行目标 = 比例 × 硬上限）
    xsynth_voice_target_ratio: f64,
    /// XSynth 过载保命闸（软 NPS 闸，默认关闭）
    xsynth_soft_nps_gate: bool,
    /// LGS (GPU) 异步初始化接收器
    lgs_init_rx: Option<Receiver<LgsInitResult>>,
    /// 是否正在异步初始化 LGS (GPU)
    is_lgs_initializing: bool,
    /// LGS (GPU) 音色库路径（用于 create_additional_output 回退创建时重用）
    lgs_soundfont_path: String,
    /// LGS (GPU) 渲染采样率（Hz）
    lgs_sample_rate: u32,
    /// LGS (GPU) 每块渲染帧数
    lgs_block_size: usize,
    /// LGS (GPU) 每个 (通道, 键) 最大同音数
    lgs_max_voices_per_key: usize,
    /// LGS (GPU) 是否使用 64 点 sinc 高质量插值
    lgs_use_sinc: bool,
    /// 系统 MIDI (WinMM) 输出设备 ID（None = 第一个/默认）
    winmm_output_device_id: Option<u32>,
    /// REND-002：当前文档期望的最大 MIDI 端口（0 = 单端口/Midi）。
    ///
    /// 由文档装载入口写入；XSynth 就绪时应用/重建，未就绪（异步初始化或
    /// System 回退）时暂存，待 `check_async_init_complete` 完成后对齐。
    desired_midi_max_port: u8,
    /// REND-002：启动本轮 XSynth 异步初始化时注入的布局；完成回调据此判断
    /// 是否需要再对齐（初始化期间文档可能已切换），避免刚初始化完又白重建一次。
    spawned_midi_max_port: u8,
    /// DEBT-05 #122：后台布局重建在途状态（一次性 worker + 防抖 + 合并）。
    layout_apply: Option<layout::LayoutApplyInFlight>,
    /// DEBT-05 #122：worker 在防抖窗口结束时读取的"最新期望布局"。
    layout_desired: Arc<AtomicU8>,
    /// #127：待向用户展示的后端提示（LGS 静默降级/初始化失败可见化）。
    ///
    /// 由 Runner 每帧取走并写入状态栏；取走即清，避免重复提示。
    pending_backend_notice: Option<String>,
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
            xsynth_sample_rate: 0,
            xsynth_max_voices_per_key: None,
            xsynth_global_voice_limit: None,
            xsynth_voice_target_ratio: 1.0 - 1.0 / std::f64::consts::E,
            xsynth_soft_nps_gate: false,
            lgs_init_rx: None,
            is_lgs_initializing: false,
            lgs_soundfont_path: String::new(),
            lgs_sample_rate: 0,
            lgs_block_size: 0,
            lgs_max_voices_per_key: 0,
            lgs_use_sinc: false,
            winmm_output_device_id: None,
            desired_midi_max_port: 0,
            spawned_midi_max_port: 0,
            layout_apply: None,
            layout_desired: Arc::new(AtomicU8::new(0)),
            pending_backend_notice: None,
        }
    }
}

impl MidiManager {
    /// 取走待展示的后端提示（#127：LGS 静默降级/初始化失败可见化）。
    ///
    /// 取走即清；Runner 取到后写入状态栏，用户可见"当前实际后端与原因"。
    pub fn take_pending_backend_notice(&mut self) -> Option<String> {
        self.pending_backend_notice.take()
    }
}

// ── 子模块 ──────────────────────────────────────────────────────────────

mod audio_action;
mod layout;
mod lgs;
mod outputs;
mod recovery;
mod xsynth;

pub use audio_action::handle_audio_action;

#[cfg(test)]
mod tests {
    use super::*;

    /// #127 问题1：未设置音色库时选择 LGS 不得静默降级——必须产生可见提示，
    /// 且不启动后台初始化（保持 System 快速启动后端）。
    #[test]
    fn test_lgs_without_soundfont_reports_notice() {
        let mut manager = MidiManager::default();
        // `UiConfig::default()` 的 soundfont_path 即空串（未设置音色库）。
        let config = UiConfig::default();

        manager.start_lgs_async_init(&config);

        let notice = manager
            .take_pending_backend_notice()
            .expect("未设置音色库时应产生可见提示");
        assert!(notice.contains("LGS"), "提示应说明 LGS 未生效: {notice}");
        assert!(
            !manager.is_lgs_initializing(),
            "未设置音色库不应启动后台初始化"
        );
        assert!(
            manager.take_pending_backend_notice().is_none(),
            "提示取走即清，不应重复出现"
        );
    }

    /// #127 问题1：音色库文件不存在同样必须产生可见提示（而非仅 WARN 日志）。
    #[test]
    fn test_lgs_missing_soundfont_file_reports_notice() {
        let mut manager = MidiManager::default();
        let config = UiConfig {
            soundfont_path: "Z:/definitely/not/here/钢琴.sfz".to_string(),
            ..UiConfig::default()
        };

        manager.start_lgs_async_init(&config);

        let notice = manager
            .take_pending_backend_notice()
            .expect("音色库文件不存在时应产生可见提示");
        assert!(notice.contains("LGS"), "提示应说明 LGS 未生效: {notice}");
        assert!(!manager.is_lgs_initializing());
    }

    /// #127 问题2：LGS 异步初始化失败必须落定为 `Failed`（而非静默 false），
    /// 并产生携带原因的可见提示，供 Runner 触发播放输出重连与状态栏展示。
    #[test]
    fn test_lgs_async_failure_reports_failed_outcome_and_notice() {
        let (tx, rx) = channel();
        let mut manager = MidiManager {
            is_lgs_initializing: true,
            lgs_init_rx: Some(rx),
            ..MidiManager::default()
        };
        tx.send(LgsInitResult::Failed("音色库加载失败".to_string()))
            .expect("测试通道发送失败结果");

        let outcome = manager.check_async_init_complete();

        assert_eq!(outcome, AsyncInitOutcome::Failed);
        assert!(!manager.is_lgs_initializing(), "失败后应结束初始化状态");
        let notice = manager
            .take_pending_backend_notice()
            .expect("初始化失败应产生可见提示");
        assert!(
            notice.contains("音色库加载失败"),
            "提示应携带失败原因: {notice}"
        );
    }

    /// 不在初始化时，落定结果为 `Pending`（调用方不应重连播放输出）。
    #[test]
    fn test_async_outcome_pending_when_not_initializing() {
        let mut manager = MidiManager::default();
        assert_eq!(
            manager.check_async_init_complete(),
            AsyncInitOutcome::Pending
        );
    }
}
