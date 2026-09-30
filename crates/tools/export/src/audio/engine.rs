//! 音频引擎初始化 — 创建和管理 xsynth ChannelGroup
//!
//! 参考 OmniConverter 的 XSynthEngine / XSynthRenderer 设计：
//! - Engine 负责加载 SoundFont 和配置
//! - Renderer 负责渲染音频样本

use xsynth_core::{
    AudioPipe,
    channel::{ChannelConfigEvent, ChannelEvent},
    channel_group::{ChannelGroup, SynthEvent},
};

use crate::error::ExportResult;

use super::config::AudioRenderConfig;
use super::event::load_soundfonts;

/// 渲染引擎 — 封装 xsynth 的 ChannelGroup
///
/// 对应 OmniConverter 的 XSynthRenderer 概念：
/// - 持有 ChannelGroup（合成引擎）
/// - 提供音频渲染能力
pub struct AudioEngine {
    channel_group: ChannelGroup,
    config: AudioRenderConfig,
}

impl AudioEngine {
    /// 创建新的音频引擎
    ///
    /// 初始化 xsynth ChannelGroup 并加载 SoundFont
    pub fn new(config: AudioRenderConfig) -> ExportResult<Self> {
        let group_config = config.build_group_config();
        let mut channel_group = ChannelGroup::new(group_config);

        // REND-002：`SynthFormat::Custom` 不自动开启 ch9 打击乐（仅 `Midi` 自动，
        // 见 fork `channel_group/mod.rs:77-81`），多端口必须对每个端口的
        // `p*16+9` 显式下发 SetPercussionMode(true)（含 port 0）。
        for channel in config.percussion_channels() {
            channel_group.send_event(SynthEvent::Channel(
                channel,
                ChannelEvent::Config(ChannelConfigEvent::SetPercussionMode(true)),
            ));
        }

        // 加载 SF2 音色库
        load_soundfonts(&mut channel_group, &config)?;

        Ok(AudioEngine {
            channel_group,
            config,
        })
    }

    /// 获取 ChannelGroup 的可变引用
    pub fn channel_group(&mut self) -> &mut ChannelGroup {
        &mut self.channel_group
    }

    /// 获取 ChannelGroup 的流参数
    pub fn stream_params(&self) -> xsynth_core::AudioStreamParams {
        *self.channel_group.stream_params()
    }

    /// 获取配置引用
    pub fn config(&self) -> &AudioRenderConfig {
        &self.config
    }

    /// 获取当前活跃 voice 数
    pub fn voice_count(&self) -> u64 {
        self.channel_group.voice_count()
    }
}
