//! REND-005：GPU 后端的端口音色库能力边界。
//!
//! `GpuSynth` 只持有一个 `SoundFont`，没有按端口切换音色库的实现路径。
//! 渲染入口必须在创建 GPU 设备前显式拒绝非空映射，避免静默忽略配置、
//! 产出“听起来不对但没人知道”的导出文件。

use lumino_core::tr;

use crate::audio::config::AudioRenderConfig;
use crate::error::{ExportError, ExportResult};

/// 非空端口映射 → 明确报错；空映射保持 GPU 导出零行为变化。
pub(super) fn reject_port_soundfonts(config: &AudioRenderConfig) -> ExportResult<()> {
    if config.port_soundfonts.is_empty() {
        return Ok(());
    }
    Err(ExportError::AudioWrite(
        tr!("GPU 后端暂不支持按端口音色库，请切换 CPU 后端或清除端口音色库映射").into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REND-005：GPU 后端不支持按端口音色库，必须在渲染前拒绝。
    #[test]
    fn gpu_rejects_port_soundfonts_mapping() {
        let mut config = AudioRenderConfig::default();
        config.port_soundfonts.set(1, vec!["port1.sf2".to_string()]);
        assert!(
            reject_port_soundfonts(&config).is_err(),
            "非空映射必须被 GPU 渲染入口拒绝"
        );
    }

    /// 空映射保持 GPU 导出零行为变化。
    #[test]
    fn gpu_accepts_empty_port_soundfonts() {
        let config = AudioRenderConfig::default();
        assert!(reject_port_soundfonts(&config).is_ok());
    }
}
