//! 全局 UI 状态更新：进度、菜单、修饰键、面板尺寸与性能数据。
//!
//! 从 `state_update.rs` 的穷尽分派表按业务对象拆出；状态读写顺序逐行保持原样。

use crate::message::PerfData;
use crate::root::Root;

impl Root {
    /// 更新进度提示（None 表示清除）
    pub(super) fn handle_progress(&mut self, progress: &Option<(String, f64)>) -> bool {
        if let Some((ref msg, p)) = *progress {
            self.progress = Some((msg.clone(), p));
        } else {
            self.progress = None;
        }
        true
    }

    /// 菜单开合状态
    pub(super) fn menu_state(&mut self, is_open: bool) -> bool {
        self.state.is_menu_open = is_open;
        true
    }

    /// Ctrl 键状态：同步到工具栏与编辑器
    pub(super) fn ctrl_key(&mut self, pressed: bool) -> bool {
        self.toolbar.ctrl_pressed = pressed;
        // 同步到 Editor：ruler/键盘区 Ctrl+滚轮缩放依赖此可靠通道
        // （上一代 canvas 内 ModifiersChanged 事件可能因焦点问题不送达）
        self.editor.set_ctrl_pressed(pressed);
        true
    }

    /// Shift 键状态：同步到工具栏与编辑器
    pub(super) fn shift_key(&mut self, pressed: bool) -> bool {
        self.toolbar.shift_pressed = pressed;
        // 同步到 Editor：形状工具拖拽绘制时实时约束正图形依赖此可靠通道
        self.editor.set_shift_pressed(pressed);
        true
    }

    /// 力度面板高度
    pub(super) fn velocity_height(&mut self, height: f32) -> bool {
        self.visual.velocity_panel_height = height;
        true
    }

    /// 性能监控数据
    pub(super) fn perf_update(&mut self, data: PerfData) -> bool {
        self.statusbar.set_perf_data(data);
        true
    }

    /// MIDI 输入：入队到输入缓冲区；锁失败时静默丢弃，与原臂语义一致
    pub(super) fn midi_input(&mut self, data: &[u8]) -> bool {
        let data = data.to_vec();
        if let Ok(mut buf) = self.midi.input_buffer.lock() {
            buf.push_back(data);
        }
        true
    }
}
