//! `MidiManager` 查询/取通知类方法（从 `midi_manager.rs` 拆分，保持文件 < 400 行）。

use super::*;

impl MidiManager {
    /// 取走待展示的后端提示（#127：LGS 静默降级/初始化失败可见化）。
    ///
    /// 取走即清；Runner 取到后写入状态栏，用户可见"当前实际后端与原因"。
    pub fn take_pending_backend_notice(&mut self) -> Option<String> {
        self.pending_backend_notice.take()
    }

    /// 取走「当前后端不支持按端口音色库」的弹窗请求（REND-005）。
    ///
    /// 取走即清；Runner 收到后打开独立提示窗（用户可继续降级或切 XSynth）。
    pub fn take_port_soundfonts_unsupported_notice(
        &mut self,
    ) -> Option<PortSoundfontsUnsupportedNotice> {
        self.pending_port_soundfonts_unsupported.take()
    }

    /// 当前实际生效的合成后端（只读，供 Runner 做 UI 分支）。
    pub fn active_backend(&self) -> SynthBackend {
        self.active_backend
    }

    /// 工程是否没有任何端口音色库覆盖（Runner 用来自动关闭过期的“不支持”提示）。
    pub fn desired_port_soundfonts_is_empty(&self) -> bool {
        self.desired_port_soundfonts.is_empty()
    }

    /// 取当前后端（仅 LGS 实现）的 GPU 缓冲状态快照。
    ///
    /// 非 LGS 后端由 `SynthControl` 默认实现返回 `None`；端口布局重建窗口期
    /// `api` 被 worker 暂持，同样返回 `None`（UI 短暂隐藏指示器）。
    pub fn gpu_buffer_stats(&self) -> Option<lumino_midi_io::GpuBufferStats> {
        self.api.as_ref()?.gpu_buffer_stats()
    }

    /// 取 Maestro 实时统计（voice_count / nps / eps / nps_limit / latency）。
    ///
    /// 非 Maestro 后端或引擎重建窗口返回 `None`；供状态栏/性能指示器低频轮询。
    pub fn maestro_stats(&self) -> Option<MaestroStats> {
        self.maestro_stats.as_ref()?.stats()
    }

    /// 取走 Maestro 引擎通知（禁用/停止），供状态栏提示。
    pub fn take_maestro_notice(&self) -> Option<String> {
        self.maestro_stats.as_ref()?.take_notice()
    }
}
