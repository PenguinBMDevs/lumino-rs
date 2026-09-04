//! 编辑器操作 - MIDI 输出管理

use crate::root::Root;

impl Root {
    /// 加载 Tempo 变化事件到播放管理器
    /// tempo_changes: Vec<(tick, tempo_in_microseconds_per_quarter_note)>
    pub fn load_tempo_changes(&mut self, tempo_changes: Vec<(u32, u32)>) {
        tracing::debug!(
            "Root::load_tempo_changes: loading {} tempo changes",
            tempo_changes.len()
        );

        let tempo_change_list: Vec<crate::playback::TempoChange> = tempo_changes
            .into_iter()
            .map(|(tick, tempo)| crate::playback::TempoChange {
                tick: tick as f32,
                tempo,
            })
            .collect();

        // 如果有播放管理器，更新其 tempo timeline
        if let Some(manager) = &mut self.playback.manager {
            manager.update_tempo_changes(tempo_change_list);
            tracing::debug!("Root::load_tempo_changes: tempo changes updated in playback manager");
        } else {
            self.playback.pending_tempo_changes = Some(tempo_change_list);
            tracing::debug!(
                "Root::load_tempo_changes: playback manager not ready, cached tempo changes"
            );
        }
    }

    /// 设置 MIDI 输出连接
    pub fn set_midi_output(&mut self, output: Box<dyn lumino_midi_io::OutputConnection>) {
        if let Some(manager) = &mut self.playback.manager {
            manager.set_midi_output(output);
            tracing::info!("Root::set_midi_output: MIDI output connection set");
        } else {
            self.playback.pending_midi_output = Some(output);
            tracing::debug!(
                "Root::set_midi_output: playback manager not ready, cached MIDI output"
            );
        }
    }

    /// 清除 MIDI 输出连接
    pub fn clear_midi_output(&mut self) {
        if let Some(manager) = &mut self.playback.manager {
            manager.clear_midi_output();
            tracing::info!("Root::clear_midi_output: MIDI output connection cleared");
        }
        self.playback.pending_midi_output = None;
    }

    /// 系统 MIDI (WinMM) 播表（输出设备）自动扫描。
    ///
    /// 通过 System API 枚举所有可用的 WinMM MIDI 输出端口，写入设置面板，
    /// 供「WinMM 输出设备」下拉菜单展示（系统播表自动扫描）。
    pub fn scan_winmm_outputs(&mut self) {
        use lumino_midi_io::ApiKind;

        match lumino_midi_io::new_api(&ApiKind::System) {
            Ok(api) => {
                let outputs = api.outputs().unwrap_or_default();
                let list: Vec<(u32, String)> =
                    outputs.iter().map(|o| (o.id, o.name.clone())).collect();
                tracing::info!("扫描到 {} 个 WinMM 输出设备(播表)", list.len());
                self.settings.midi.winmm_outputs = list.clone();

                // 校验已选设备是否仍然有效；失效则回落到第一个（None = 使用系统默认）
                if let Some(sel) = self.settings.midi.selected_winmm_output
                    && !list.iter().any(|(id, _)| *id == sel)
                {
                    self.settings.midi.selected_winmm_output = list.first().map(|(id, _)| *id);
                }
            }
            Err(e) => {
                tracing::warn!("扫描 WinMM 输出设备(播表)失败: {}", e);
                self.settings.midi.winmm_outputs = Vec::new();
            }
        }
    }

    /// 音频播放输出设备（CPAL 音频设备）自动扫描。
    ///
    /// 通过 CPAL 枚举所有可用的音频输出设备，写入设置面板，
    /// 供「音频播放输出设备」下拉菜单展示（系统音频设备自动扫描）。
    pub fn scan_audio_outputs(&mut self) {
        let list = lumino_midi_io::audio_devices::enumerate_audio_output_devices();
        tracing::info!("扫描到 {} 个音频播放输出设备", list.len());
        self.settings.synth.audio_output_devices = list.clone();

        // 校验已选设备是否仍然有效；失效则回落到系统默认（None）
        if let Some(sel) = self.settings.synth.selected_audio_output_device.clone()
            && !list.contains(&sel)
        {
            self.settings.synth.selected_audio_output_device = None;
        }
    }
}

#[cfg(test)]
mod tests;
