//! 音色库路径校验与加载
//!
//! 自 `event.rs` 拆分而来（零逻辑变更）。

use std::sync::Arc;

use xsynth_core::{
    AudioPipe,
    channel::{ChannelConfigEvent, ChannelEvent},
    channel_group::{ChannelGroup, SynthEvent},
    soundfont::{SampleSoundfont, SoundfontBase},
};

use crate::audio::config::{AudioRenderConfig, normalize_layer_limit};
use crate::error::{ExportError, ExportResult};

fn validate_sf2_path(path: &std::path::Path) -> ExportResult<()> {
    if !path.exists() {
        return Err(ExportError::AudioWrite(format!(
            "音色库文件不存在: {:?}",
            path
        )));
    }
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        if ext.eq_ignore_ascii_case("sfz") {
            // CPU 的 SampleSoundfont 支持 SFZ（按扩展名分发到 new_sfz），允许通过；
            // 仅校验文件可读，不做 RIFF 检查
            return Ok(());
        }
        if !ext.eq_ignore_ascii_case("sf2") {
            return Err(ExportError::AudioWrite(format!(
                "不支持的音色库格式 {:?}：仅支持 .sf2/.sfz",
                path
            )));
        }
    }
    // 仅对 SF2 快速校验 RIFF 头，避免底层 soundfont crate 直接 panic（abort）
    if let Some(ext) = path.extension().and_then(|s| s.to_str())
        && ext.eq_ignore_ascii_case("sf2")
        && let Ok(mut f) = std::fs::File::open(path)
    {
        use std::io::Read;
        let mut header = [0u8; 4];
        if f.read_exact(&mut header).is_ok() && header != *b"RIFF" {
            return Err(ExportError::AudioWrite(format!(
                "音色库不是合法的 SF2 文件 {:?}：头应为 RIFF，实际 {:02X?}",
                path, header
            )));
        }
    }
    Ok(())
}

/// 加载 SF2 音色库到 ChannelGroup
pub fn load_soundfonts(
    channel_group: &mut ChannelGroup,
    config: &AudioRenderConfig,
) -> ExportResult<()> {
    if config.soundfonts.is_empty() {
        return Err(ExportError::AudioWrite(
            "未指定音色库文件，请在导出面板选择 .sf2".into(),
        ));
    }
    for p in &config.soundfonts {
        validate_sf2_path(p)?;
    }

    let stream_params = *channel_group.stream_params();
    let sf_options = config.build_sf_options();

    let soundfonts: Vec<Arc<dyn SoundfontBase>> = config
        .soundfonts
        .iter()
        .map(|sf_path| {
            // 使用音色库标签追踪每个音色库加载时的内存分配
            lumino_diagnostics::memtrace::with_tag(
                lumino_diagnostics::memtrace::AllocTag::SoundFont,
                || {
                    let sf: Arc<dyn SoundfontBase> = Arc::new(
                        SampleSoundfont::new(sf_path, stream_params, sf_options).map_err(|e| {
                            ExportError::AudioWrite(format!("音色库 {sf_path:?}: {e}"))
                        })?,
                    );
                    Ok(sf)
                },
            )
        })
        .collect::<ExportResult<Vec<_>>>()?;

    channel_group.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
        ChannelConfigEvent::SetSoundfonts(soundfonts),
    )));
    channel_group.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
        // 归一化后再下发：`Some(0)` 绝不能直传 xsynth（会退化成"每键只保留
        // 最新一组"，该键其余音符全部无声），GPU 侧走同一个 helper。
        ChannelConfigEvent::SetLayerCount(normalize_layer_limit(config.layer_limit)),
    )));

    Ok(())
}
