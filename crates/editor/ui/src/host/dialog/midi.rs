//! 设置对话框 MIDI 设备刷新入口。
//!
//! 独立成文件：`host/dialog/state.rs` 已超过 600 行，新增的设备刷新逻辑
//! 不应继续膨胀该文件。

use crate::host::Host;

impl Host {
    /// 主动刷新 MIDI 输入设备列表（设置窗重新获得焦点 / 点「刷新」）。
    ///
    /// 扫描在后台线程执行；这里安排一次短延迟重绘驱动结果轮询，
    /// 避免扫描完成时没有窗口事件、列表一直不更新。
    pub fn refresh_midi_input_devices(&mut self) {
        self.root.scan_midi_input_devices();
        self.last_midi_devices_scan = Some(std::time::Instant::now());
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
        self.schedule_redraw_at(std::time::Instant::now() + std::time::Duration::from_millis(50));
    }
}
