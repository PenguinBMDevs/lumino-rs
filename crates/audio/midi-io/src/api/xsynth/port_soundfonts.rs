//! REND-005：XSynth 实时后端的“端口 → 音色库”覆盖。
//!
//! 语义与导出侧一致：
//! 1. 先对全部通道下发全局回退库；
//! 2. 再对每个已配置端口的 16 个全局通道下发覆盖库（优先级回退链）。
//!
//! 运行期切换只换音色库矩阵：不重建管线、不重开音频流、不发 `SystemReset`、
//! 不杀音。`SetSoundfonts` 本身不会终止正在发声的 voice，但 NoteOff 的 release
//! spawner 使用**当前**矩阵，因此调用方必须保证“停播后再切换”。

use std::path::PathBuf;
use std::sync::Arc;

use xsynth_core::{
    AudioStreamParams,
    channel::{ChannelConfigEvent, ChannelEvent},
    soundfont::SoundfontBase,
};

use lumino_core::trf;
use lumino_midi_model::multi_port::{PortSoundfontMap, channels_for_port};

use crate::realtime::{RealtimeEventSender, SynthEvent};
use crate::soundfont_cache::load_soundfont_cached;

use super::XSynth;

/// 端口覆盖计划：`(port, paths)`，仅包含 `port <= max_port` 的有效条目。
///
/// 纯函数：不做文件系统访问，供初始化/运行期共用与单测。空 `paths`
/// 条目在模型层已被移除，这里仍过滤一次，防御未来直构条目的调用方。
pub(super) fn port_soundfont_plan(map: &PortSoundfontMap, max_port: u8) -> Vec<(u8, Vec<PathBuf>)> {
    map.entries_for_max_port(max_port)
        .filter(|entry| !entry.paths.is_empty())
        .map(|entry| {
            (
                entry.port,
                entry.paths.iter().map(PathBuf::from).collect::<Vec<_>>(),
            )
        })
        .collect()
}

/// 一次端口音色库下发的统计与提示。
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct PortSoundfontApply {
    /// 成功下发的端口覆盖数。
    pub applied_ports: usize,
    /// 加载失败/回退全局的可见提示（空 = 完全成功）。
    pub warnings: Vec<String>,
}

/// 加载并下发全局库 + 端口覆盖（初始化与运行期共用）。
///
/// 返回的 `warnings` 非空表示部分端口已回退全局；调用方负责展示提示。
pub(super) fn send_port_soundfonts(
    sender: &mut RealtimeEventSender,
    global: Arc<dyn SoundfontBase>,
    map: &PortSoundfontMap,
    max_port: u8,
    params: AudioStreamParams,
) -> PortSoundfontApply {
    let plan = port_soundfont_plan(map, max_port);
    let mut warnings = Vec::new();
    let mut loaded: Vec<(u8, Vec<Arc<dyn SoundfontBase>>)> = Vec::with_capacity(plan.len());

    for (port, paths) in plan {
        let mut fonts = Vec::with_capacity(paths.len());
        for path in &paths {
            match load_soundfont_cached(path, params) {
                Ok(font) => fonts.push(font),
                Err(error) => warnings.push(trf!(
                    "端口 {port} 的音色库 {path} 加载失败，已回退全局：{error}",
                    port = port,
                    path = format!("{path:?}"),
                    error = error
                )),
            }
        }
        if fonts.is_empty() {
            // 该端口全部失败：保持全局回退（随后 AllChannels 已覆盖）。
            continue;
        }
        loaded.push((port, fonts));
    }

    // 1) 全局回退：先用全局库覆盖所有通道（与历史行为一致，空映射时零额外事件）。
    sender.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
        ChannelConfigEvent::SetSoundfonts(vec![global]),
    )));
    // 2) 端口覆盖：逐通道下发该端口的库（只覆盖当前布局内存在的通道）。
    for (port, fonts) in &loaded {
        for channel in channels_for_port(*port) {
            sender.send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Config(ChannelConfigEvent::SetSoundfonts(fonts.clone())),
            ));
        }
    }

    PortSoundfontApply {
        applied_ports: loaded.len(),
        warnings,
    }
}

impl XSynth {
    /// 运行期应用端口音色库（[`crate::SynthControl::apply_port_soundfonts`] 实现体）。
    pub(super) fn apply_port_soundfonts_runtime(
        &mut self,
        map: &PortSoundfontMap,
    ) -> Result<(), String> {
        let params = self.synth.stream_params();
        let global = load_soundfont_cached(&self.soundfont_path, params)
            .map_err(|error| trf!("全局音色库加载失败：{error}", error = error))?;

        let result = {
            let mut sender = self
                .sender_shared
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            send_port_soundfonts(&mut sender, global, map, self.midi_max_port, params)
        };

        // 布局重建复用本字段；即使部分端口回退全局，也记录期望值——
        // 用户修正文件后重新提交映射即可再次尝试。
        self.port_soundfonts = map.clone();

        if result.warnings.is_empty() {
            tracing::info!(
                "XSynth: 端口音色库已应用（端口覆盖 {} 个，max_port={}）",
                result.applied_ports,
                self.midi_max_port
            );
            Ok(())
        } else {
            let message = result.warnings.join("；");
            tracing::warn!("XSynth: 端口音色库部分回退全局：{message}");
            Err(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_midi_model::multi_port::PortSoundfontMap;

    fn map_with(entries: &[(u8, &[&str])]) -> PortSoundfontMap {
        let mut map = PortSoundfontMap::default();
        for (port, paths) in entries {
            map.set(
                *port,
                paths.iter().map(|path| (*path).to_string()).collect(),
            );
        }
        map
    }

    /// 空映射：不产生任何端口计划（初始化事件序列保持历史行为）。
    #[test]
    fn empty_map_produces_no_overrides() {
        let map = PortSoundfontMap::default();
        assert!(port_soundfont_plan(&map, 7).is_empty());
    }

    /// 端口 0 允许覆盖全局。
    #[test]
    fn port_zero_is_planned() {
        let map = map_with(&[(0, &["port0.sf2"])]);
        assert_eq!(
            port_soundfont_plan(&map, 0),
            vec![(0, vec![PathBuf::from("port0.sf2")])]
        );
    }

    /// 超出文档布局的端口不参与本次下发（布局扩展后由重建补齐）。
    #[test]
    fn ports_beyond_max_port_are_ignored() {
        let map = map_with(&[(1, &["port1.sf2"]), (6, &["port6.sf2"])]);
        assert_eq!(
            port_soundfont_plan(&map, 1),
            vec![(1, vec![PathBuf::from("port1.sf2")])],
            "max_port=1 时只应下发端口 0/1"
        );
    }

    /// 超出产品上限的端口按模型语义折叠到端口 15。
    #[test]
    fn ports_clamp_to_product_limit() {
        let map = map_with(&[(127, &["high.sf2"])]);
        assert_eq!(
            port_soundfont_plan(&map, 127),
            vec![(15, vec![PathBuf::from("high.sf2")])]
        );
    }

    /// 计划按端口升序、保持每条路径的优先级回退顺序。
    #[test]
    fn plan_is_sorted_and_keeps_priority_order() {
        let map = map_with(&[(2, &["b.sf2"]), (0, &["a.sf2", "fallback.sf2"])]);
        assert_eq!(
            port_soundfont_plan(&map, 2),
            vec![
                (
                    0,
                    vec![PathBuf::from("a.sf2"), PathBuf::from("fallback.sf2")]
                ),
                (2, vec![PathBuf::from("b.sf2")]),
            ]
        );
    }
}
