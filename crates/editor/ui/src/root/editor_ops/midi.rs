//! 编辑器操作 - MIDI 输出管理

use crate::root::Root;
use lumino_core::storage::config::SynthBackend;
use std::sync::Arc;
use std::sync::mpsc;

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

    /// 设置 MIDI 输出连接（含播放能力扩展）
    pub fn set_midi_output(&mut self, output: Box<dyn lumino_midi_io::PlaybackOutput>) {
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

    /// 同步清除 MIDI 输出连接：等待播放线程确认旧连接已释放后才返回（#127）。
    ///
    /// 重初始化 MIDI 前必须确保旧连接（尤其是 WinMM 端口）已真正 drop，
    /// 否则同进程二次打开同一端口必然失败。返回 `false` 表示等待超时或
    /// 播放线程已停止（无法确认释放）；无播放管理器时视为已释放（`true`）。
    pub fn clear_midi_output_sync(&mut self) -> bool {
        let released = match &mut self.playback.manager {
            Some(manager) => manager.clear_midi_output_sync(),
            None => true,
        };
        self.playback.pending_midi_output = None;
        if released {
            tracing::info!("Root::clear_midi_output_sync: MIDI output connection released");
        } else {
            tracing::warn!("Root::clear_midi_output_sync: 超时未确认 MIDI 输出连接释放");
        }
        released
    }

    /// 系统 MIDI (WinMM) 播表（输出设备）自动扫描（后台线程）。
    ///
    /// 仅对 System 后端有意义：非 System 后端直接返回（守卫，防误触发）；
    /// midir/WinMM 客户端创建与端口枚举**全部在后台线程执行**，结果经
    /// [`Root::poll_winmm_scan`] 回写设置面板（避免 UI 线程触碰 midir、
    /// 拖慢启动与设置页打开）。扫描在途时忽略重复请求。
    pub fn scan_winmm_outputs(&mut self) {
        use lumino_midi_io::ApiKind;

        if self.settings.synth.backend != SynthBackend::System {
            tracing::debug!("WinMM 播表扫描：当前后端非 System，跳过");
            return;
        }
        if self.pending_winmm_scan.is_some() {
            tracing::debug!("WinMM 播表扫描已在进行中，忽略重复请求");
            return;
        }

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let list: Vec<(u32, String)> = match lumino_midi_io::new_api(&ApiKind::System) {
                Ok(api) => api
                    .outputs()
                    .unwrap_or_default()
                    .iter()
                    .map(|o| (o.id, o.name.clone()))
                    .collect(),
                Err(e) => {
                    tracing::warn!("扫描 WinMM 输出设备(播表)失败: {}", e);
                    Vec::new()
                }
            };
            let _ = tx.send(list);
        });
        self.pending_winmm_scan = Some(rx);
    }

    /// 轮询 WinMM 播表扫描结果（后台完成 → 写回设置面板 + 校验回落）。
    pub(crate) fn poll_winmm_scan(&mut self) {
        let Some(rx) = self.pending_winmm_scan.as_ref() else {
            return;
        };
        let list = match rx.try_recv() {
            Ok(list) => list,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("WinMM 播表扫描线程异常断开");
                Vec::new()
            }
        };
        self.pending_winmm_scan = None;
        tracing::info!("扫描到 {} 个 WinMM 输出设备(播表)", list.len());
        self.settings.midi.winmm_outputs = list.clone();

        // 校验已选设备是否仍然有效；失效则回落到第一个（None = 使用系统默认）
        if let Some(sel) = self.settings.midi.selected_winmm_output
            && !list.iter().any(|(id, _)| *id == sel)
        {
            self.settings.midi.selected_winmm_output = list.first().map(|(id, _)| *id);
        }
    }

    /// MIDI 输入设备扫描（后台线程）。
    ///
    /// 枚举已注入的输入 API（`set_midi_api` 的实例，`Arc` 共享；枚举本身
    /// 会触发 midir 惰性创建，故必须在后台线程执行）。结果经
    /// [`Root::poll_midi_devices_scan`] 回写设备列表并自动选中首个设备。
    /// 扫描在途时忽略重复请求；未注入 API 时置空列表。
    pub fn scan_midi_input_devices(&mut self) {
        if self.pending_midi_devices_scan.is_some() {
            tracing::debug!("MIDI 输入设备扫描已在进行中，忽略重复请求");
            return;
        }
        let Some(api) = self.midi.api.as_ref().map(Arc::clone) else {
            tracing::debug!("MIDI 输入设备扫描：API 未注入，置空设备列表");
            self.settings.midi.devices = Vec::new();
            return;
        };

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let list: Vec<(u32, String)> = api
                .inputs()
                .unwrap_or_default()
                .iter()
                .map(|d| (d.id, d.name.clone()))
                .collect();
            let _ = tx.send(list);
        });
        self.pending_midi_devices_scan = Some(rx);
    }

    /// 轮询 MIDI 输入设备扫描结果（写回设备列表 + 自动选中首个）。
    pub(crate) fn poll_midi_devices_scan(&mut self) {
        let Some(rx) = self.pending_midi_devices_scan.as_ref() else {
            return;
        };
        let devices = match rx.try_recv() {
            Ok(devices) => devices,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("MIDI 输入设备扫描线程异常断开");
                Vec::new()
            }
        };
        self.pending_midi_devices_scan = None;
        tracing::info!("扫描到 {} 个 MIDI 输入设备", devices.len());
        self.settings.midi.devices = devices.clone();

        // 自动选中第一个设备（如果还没有选中）
        if self.settings.midi.selected_device.is_none()
            && let Some(first) = devices.first()
        {
            self.settings.midi.selected_device = Some(first.0);
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
