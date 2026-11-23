//! 用户主动触发的后台任务轮询（设备扫描 / 录制输入）。
//!
//! 这些任务的结果不会产生窗口事件，而事件循环是 `ControlFlow::Wait`：
//! 不在这里安排定时重绘，扫描完成和 MIDI 输入都会停在「状态已变、画面没变」。

use std::time::{Duration, Instant};

use super::Host;

impl Host {
    /// 轮询用户主动触发的后台设备扫描。
    ///
    /// 结果写回时置脏并请求重绘；扫描在途时安排短延迟重绘继续驱动轮询
    /// （`ControlFlow::Wait` 下没有鼠标事件就永远不会再进 `route_message`）。
    pub(super) fn poll_background_scans(&mut self) {
        let winmm_changed = self.root.poll_winmm_scan();
        let midi_changed = self.root.poll_midi_devices_scan();
        if winmm_changed || midi_changed {
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
        }
        if self.root.pending_winmm_scan.is_some() || self.root.pending_midi_devices_scan.is_some() {
            self.schedule_redraw_at(Instant::now() + Duration::from_millis(50));
        }
    }

    /// 录制期主动消费 MIDI 输入并驱动重绘。
    ///
    /// 输入来自 midir 回调线程，不会产生窗口事件；不在这里持续调度，
    /// 用户不移动鼠标时音符就不会实时出现在卷帘上（原「点了录制没反应」根因之一）。
    pub(super) fn poll_recording_input(&mut self) {
        self.root.poll_midi_input();
        if self.root.recording.is_recording {
            self.schedule_redraw_at(Instant::now() + Duration::from_millis(16));
        }
    }

    /// 设置窗音频页的热插拔自动刷新（3 秒节流，仅设置窗口生效）。
    ///
    /// 用户插拔 MIDI 键盘不会产生窗口事件；这里在音频页可见期间周期扫描，
    /// 让「无可用设备」自动变成新设备。扫描仍在后台线程，且只在该页可见时运行。
    pub(super) fn maintain_settings_device_scan(&mut self) {
        if self.root.state.dialog_type != crate::state::root_state::DialogType::Settings
            || self.root.settings.selected_menu_index != crate::settings::MENU_INDEX_AUDIO
            || self.root.midi.api.is_none()
        {
            return;
        }
        const INTERVAL: Duration = Duration::from_secs(3);
        let now = Instant::now();
        if self.root.pending_midi_devices_scan.is_some() {
            self.last_midi_devices_scan = Some(now);
            self.schedule_redraw_at(now + Duration::from_millis(50));
            return;
        }
        let due = self
            .last_midi_devices_scan
            .is_none_or(|last| now.duration_since(last) >= INTERVAL);
        if due {
            self.last_midi_devices_scan = Some(now);
            self.root.scan_midi_input_devices();
            self.schedule_redraw_at(now + Duration::from_millis(100));
        } else if let Some(last) = self.last_midi_devices_scan {
            self.schedule_redraw_at(last + INTERVAL);
        }
    }
}
