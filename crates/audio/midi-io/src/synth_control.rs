//! 合成器控制能力扩展（REND-002 / REND-005）。
//!
//! 基础 `Api` / `OutputConnection` 保持冻结：后端可选能力集中在本扩展 trait，
//! 默认实现即“能力不支持”声明，避免既有后端被迫实现。新增能力必须带默认
//! 实现，任何 required 方法改动都需单独决策（见 `decisions/2026-11-19-*`）。

use lumino_core::tr;
use lumino_midi_model::multi_port::PortSoundfontMap;

use crate::{Api, ApiKind, Error, GpuBufferStats, api};

/// 合成器控制能力扩展（REND-002，接口演进 B：基础 `Api` 冻结，能力进扩展 trait）。
///
/// 所有 `Api` 实现需显式 `impl SynthControl`（空 impl = 接受默认行为），
/// 使“后端能力支持矩阵”在代码里可见；仅软件合成后端覆写非通用能力。
pub trait SynthControl: Api {
    /// 设置实时合成管线的 MIDI 端口布局（REND-002）。
    ///
    /// `max_port` 为当前文档使用到的最大 FF 21 端口（0 = 单端口）。支持多端口
    /// 通道空间的后端（XSynth）会据此重建合成管线；默认 no-op（后端不支持多端口，
    /// 保持 16 通道折叠语义，见 C1 决策 a）。
    fn set_midi_port_layout(&mut self, _max_port: u8) -> Result<(), String> {
        Ok(())
    }

    /// 对正在运行的合成管线应用“端口 → 音色库”映射（REND-005 / DEC-002）。
    ///
    /// 默认返回 `Err`（后端不支持）。实现约束：
    /// - 不得重建合成管线、不得重开音频流、不得发 `SystemReset`、不得杀音；
    /// - 只允许更新各通道的音色库矩阵；
    /// - 部分端口加载失败时应回退全局，并在 `Err` 中携带可见原因。
    fn apply_port_soundfonts(&mut self, _map: &PortSoundfontMap) -> Result<(), String> {
        Err(tr!("当前后端不支持按端口音色库").to_string())
    }

    /// GPU (LGS) 播放缓冲状态快照（默认 `None` = 后端无此能力）。
    ///
    /// 无锁 relaxed 原子读取，供 UI 低频（2–5Hz）展示队列/负载/声部/欠载；
    /// 外部 MIDI 设备与未实现此能力的后端返回 `None`（UI 隐藏指示器）。
    fn gpu_buffer_stats(&self) -> Option<GpuBufferStats> {
        None
    }

    /// 重建 GPU 合成管线（REND-019 PR-D §1.5 L2：TDR 后热替换）。
    ///
    /// 默认 no-op：非 GPU 后端（System/XSynth）不因 GPU device 丢失而失效。
    /// LGS 覆写为两阶段：先建新引擎（此阶段失败即返回、旧管线完全不动），
    /// **成功才**停旧流并热替换事件/控制发送器（既有输出连接自动跟随）；
    /// 若停旧流后新流启动失败，返回 Err 且旧流已停，由调用方退避重试恢复。
    fn rebuild_gpu_pipeline(&mut self) -> Result<(), String> {
        Ok(())
    }
}

/// 使用默认选项创建指定类型的后端（返回含能力扩展的句柄）
pub fn new_api(kind: &ApiKind) -> Result<Box<dyn SynthControl>, Error> {
    new_api_with_options(kind, None)
}

/// 使用自定义选项创建指定类型的后端（返回含能力扩展的句柄）
pub fn new_api_with_options(
    kind: &ApiKind,

    #[allow(unused_variables)] options: Option<api::xsynth::XSynthOptions>,
) -> Result<Box<dyn SynthControl>, Error> {
    let engine: Box<dyn SynthControl> = match kind {
        ApiKind::XSynth { soundfont_path } => Box::new(api::XSynth::new(soundfont_path, options)?),
        ApiKind::Kdmapi { path } => Box::new(api::Kdmapi::new(path)?),
        ApiKind::Maestro {
            app_id,
            midi_max_port,
        } => Box::new(api::Maestro::new(app_id, *midi_max_port)?),
        #[cfg(not(target_os = "android"))]
        ApiKind::System => Box::new(api::System::new()?),
        #[cfg(target_os = "android")]
        ApiKind::System => {
            return Err(Error::InitFailed(
                "系统 MIDI 后端（midir）在 Android 上不可用，请改用 XSynth / LGS".to_string(),
            ));
        }
        ApiKind::Lgs {
            soundfont_path,
            sample_rate,
            block_size,
            max_voices_per_key,
            max_voices,
            soft_nps_gate,
            use_sinc,
            velocity_filter_threshold,
            audio_output_device,
            midi_max_port,
        } => Box::new(api::Lgs::new(
            soundfont_path,
            &api::LgsOptions {
                sample_rate: *sample_rate,
                block_size: *block_size,
                max_voices_per_key: *max_voices_per_key,
                max_voices: *max_voices,
                soft_nps_gate: *soft_nps_gate,
                use_sinc: *use_sinc,
                velocity_filter_threshold: *velocity_filter_threshold,
                audio_output_device: audio_output_device.clone(),
                midi_max_port: *midi_max_port,
            },
        )?),
    };
    Ok(engine)
}
