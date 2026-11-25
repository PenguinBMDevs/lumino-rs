//! 音色库路径校验与加载（含 REND-005 端口覆盖）
//!
//! 自 `event.rs` 拆分而来。加载顺序：
//! 1. 全局 `soundfonts` 下发到所有通道（历史行为）；
//! 2. 每个已配置端口的 16 个全局通道逐通道下发覆盖库；
//! 3. 最后统一下发每键层数。
//!
//! 空映射时第 2 步不产生任何事件，事件序列与历史完全一致。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use xsynth_core::{
    AudioPipe,
    channel::{ChannelConfigEvent, ChannelEvent},
    channel_group::{ChannelGroup, SynthEvent},
    soundfont::{SampleSoundfont, SoundfontBase},
};

use lumino_core::{tr, trf};
use lumino_midi_model::multi_port::channels_for_port;

use crate::audio::config::{AudioRenderConfig, normalize_layer_limit};
use crate::error::{ExportError, ExportResult};

/// 导出侧音色库计划（REND-005）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SoundfontPlan {
    /// 全局回退库（未配置端口使用）。
    pub global: Vec<PathBuf>,
    /// 端口覆盖（按端口升序）。
    pub ports: Vec<(u8, Vec<PathBuf>)>,
    /// 去重后的全部路径（全局 + 覆盖，保持首次出现顺序）。
    pub distinct: Vec<PathBuf>,
}

/// 根据配置生成加载/下发计划（纯函数，便于单测）。
pub(crate) fn build_soundfont_plan(config: &AudioRenderConfig) -> SoundfontPlan {
    let global = config.soundfonts.clone();
    let mut ports = Vec::new();
    let mut distinct = Vec::with_capacity(global.len());

    for path in &global {
        push_distinct(&mut distinct, path);
    }
    for entry in config
        .port_soundfonts
        .entries_for_max_port(config.midi_max_port)
    {
        let paths: Vec<PathBuf> = entry
            .paths
            .iter()
            .map(|path| PathBuf::from(path.as_str()))
            .collect();
        if paths.is_empty() {
            // `PortSoundfontMap` 已过滤空 paths；此处仅防御未来直构条目的调用方。
            continue;
        }
        for path in &paths {
            push_distinct(&mut distinct, path);
        }
        ports.push((entry.port, paths));
    }

    SoundfontPlan {
        global,
        ports,
        distinct,
    }
}

fn push_distinct(paths: &mut Vec<PathBuf>, path: &Path) {
    if !paths.iter().any(|existing| existing.as_path() == path) {
        paths.push(path.to_path_buf());
    }
}

fn validate_sf2_path(path: &Path) -> ExportResult<()> {
    if !path.exists() {
        return Err(ExportError::AudioWrite(trf!(
            "音色库文件不存在: {path}",
            path = format!("{path:?}")
        )));
    }
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        if ext.eq_ignore_ascii_case("sfz") {
            // CPU 的 SampleSoundfont 支持 SFZ（按扩展名分发到 new_sfz），允许通过；
            // 仅校验文件可读，不做 RIFF 检查
            return Ok(());
        }
        if !ext.eq_ignore_ascii_case("sf2") {
            return Err(ExportError::AudioWrite(trf!(
                "不支持的音色库格式 {path}：仅支持 .sf2/.sfz",
                path = format!("{path:?}")
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
            return Err(ExportError::AudioWrite(trf!(
                "音色库不是合法的 SF2 文件 {path}：头应为 RIFF，实际 {header}",
                path = format!("{path:?}"),
                header = format!("{header:02X?}")
            )));
        }
    }
    Ok(())
}

/// 加载音色库到 ChannelGroup（全局 + 端口覆盖）。
pub fn load_soundfonts(
    channel_group: &mut ChannelGroup,
    config: &AudioRenderConfig,
) -> ExportResult<()> {
    if config.soundfonts.is_empty() {
        return Err(ExportError::AudioWrite(
            tr!("未指定音色库文件，请在导出面板选择 .sf2").into(),
        ));
    }

    let plan = build_soundfont_plan(config);
    for path in &plan.distinct {
        validate_sf2_path(path)?;
    }

    let stream_params = *channel_group.stream_params();
    let sf_options = config.build_sf_options();
    let started = std::time::Instant::now();

    // 同一路径只解析一次；全局与端口覆盖共享同一个 Arc。
    let mut loaded: HashMap<PathBuf, Arc<dyn SoundfontBase>> =
        HashMap::with_capacity(plan.distinct.len());
    for sf_path in &plan.distinct {
        // 使用音色库标签追踪每个音色库加载时的内存分配
        let sf = lumino_diagnostics::memtrace::with_tag(
            lumino_diagnostics::memtrace::AllocTag::SoundFont,
            || {
                let sf: Arc<dyn SoundfontBase> = Arc::new(
                    SampleSoundfont::new(sf_path, stream_params, sf_options).map_err(|e| {
                        ExportError::AudioWrite(trf!(
                            "音色库 {sf_path}: {e}",
                            sf_path = format!("{sf_path:?}"),
                            e = e
                        ))
                    })?,
                );
                Ok::<Arc<dyn SoundfontBase>, ExportError>(sf)
            },
        )?;
        loaded.insert(sf_path.clone(), sf);
    }

    // 1) 全局回退：所有通道先指向全局库（与历史行为一致）。
    let global_soundfonts: Vec<Arc<dyn SoundfontBase>> = plan
        .global
        .iter()
        .map(|path| Arc::clone(&loaded[path]))
        .collect();
    channel_group.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
        ChannelConfigEvent::SetSoundfonts(global_soundfonts),
    )));

    // 2) 端口覆盖：逐通道下发该端口的库（空映射时零事件）。
    for (port, paths) in &plan.ports {
        let port_soundfonts: Vec<Arc<dyn SoundfontBase>> =
            paths.iter().map(|path| Arc::clone(&loaded[path])).collect();
        for channel in channels_for_port(*port) {
            channel_group.send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Config(ChannelConfigEvent::SetSoundfonts(port_soundfonts.clone())),
            ));
        }
    }

    // 3) 层数保持全局配置。
    channel_group.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
        // 归一化后再下发：`Some(0)` 绝不能直传 xsynth（会退化成"每键只保留
        // 最新一组"，该键其余音符全部无声），GPU 侧走同一个 helper。
        ChannelConfigEvent::SetLayerCount(normalize_layer_limit(config.layer_limit)),
    )));

    tracing::info!(
        "音色库加载完成：全局 {} 个，端口覆盖 {} 个，去重后 {} 个文件（耗时 {}ms）",
        plan.global.len(),
        plan.ports.len(),
        plan.distinct.len(),
        started.elapsed().as_millis()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(
        global: &[&str],
        ports: &[(u8, &[&str])],
        midi_max_port: u8,
    ) -> AudioRenderConfig {
        let mut config = AudioRenderConfig {
            soundfonts: global.iter().map(PathBuf::from).collect(),
            midi_max_port,
            ..Default::default()
        };
        for (port, paths) in ports {
            config.port_soundfonts.set(
                *port,
                paths.iter().map(|path| (*path).to_string()).collect(),
            );
        }
        config
    }

    #[test]
    fn empty_mapping_keeps_global_only() {
        let config = config_with(&["global.sf2"], &[], 0);
        let plan = build_soundfont_plan(&config);
        assert_eq!(plan.global, vec![PathBuf::from("global.sf2")]);
        assert!(plan.ports.is_empty(), "空映射不得产生端口覆盖事件");
        assert_eq!(plan.distinct, vec![PathBuf::from("global.sf2")]);
    }

    #[test]
    fn port_mapping_adds_override_paths() {
        let config = config_with(&["global.sf2"], &[(1, &["port1.sf2"])], 1);
        let plan = build_soundfont_plan(&config);
        assert_eq!(
            plan.ports,
            vec![(1, vec![PathBuf::from("port1.sf2")])],
            "端口 1 应产生一条覆盖计划"
        );
        assert_eq!(
            plan.distinct,
            vec![PathBuf::from("global.sf2"), PathBuf::from("port1.sf2")],
            "去重集应包含全局与端口路径"
        );
    }

    #[test]
    fn port_zero_can_override_global() {
        let config = config_with(&["global.sf2"], &[(0, &["port0.sf2"])], 0);
        let plan = build_soundfont_plan(&config);
        assert_eq!(plan.ports, vec![(0, vec![PathBuf::from("port0.sf2")])]);
    }

    #[test]
    fn ports_beyond_max_port_are_ignored() {
        let config = config_with(&["global.sf2"], &[(1, &["port1.sf2"])], 0);
        let plan = build_soundfont_plan(&config);
        assert!(
            plan.ports.is_empty(),
            "max_port=0 时端口 1 的覆盖不应参与本次导出"
        );
    }

    #[test]
    fn distinct_paths_dedupe_global_and_port() {
        let config = config_with(
            &["shared.sf2"],
            &[(1, &["shared.sf2", "port1.sf2"]), (2, &["port1.sf2"])],
            2,
        );
        let plan = build_soundfont_plan(&config);
        assert_eq!(
            plan.distinct,
            vec![PathBuf::from("shared.sf2"), PathBuf::from("port1.sf2")],
            "全局与多个端口重复引用同一路径时应只加载一次"
        );
    }

    #[test]
    fn plan_ports_are_sorted_and_clamped() {
        let config = config_with(&["global.sf2"], &[(16, &["p15.sf2"])], 16);
        let plan = build_soundfont_plan(&config);
        assert_eq!(
            plan.ports,
            vec![(15, vec![PathBuf::from("p15.sf2")])],
            "端口 16 应折叠到端口 15"
        );
    }
}
