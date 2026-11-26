//! 仪表盘（BPM / PPQ）编辑与自动滚动模式。

use lumino_core::shape::expr::is_int_input as is_expression_input;

use super::*;

impl Toolbar {
    pub(super) fn update_dashboard(&mut self, event: Event) {
        match event {
            Event::PpqEditToggled(current_ppq) => {
                if self.ppq_editing {
                    // 已在编辑状态 → 取消编辑
                    self.ppq_editing = false;
                    self.ppq_edit_buffer.clear();
                    self.ppq_edit_error = None;
                } else {
                    // 进入编辑状态，用当前 PPQ 值初始化缓冲区
                    self.ppq_editing = true;
                    self.ppq_edit_buffer = current_ppq.to_string();
                    self.ppq_edit_error = None;
                    // 仪表盘同一时间只编辑一个字段
                    self.bpm_editing = false;
                    self.bpm_edit_buffer.clear();
                    self.bpm_edit_error = None;
                }
            }
            Event::PpqEditChanged(value) => {
                if self.ppq_editing {
                    // 允许数字与整数表达式字符；非法字符直接丢弃，保留上一次合法输入。
                    if value.is_empty() || is_expression_input(&value) {
                        self.ppq_edit_buffer = value;
                        self.ppq_edit_error = None;
                    }
                }
            }
            Event::PpqEditConfirmed => {
                self.ppq_editing = false;
                self.ppq_edit_buffer.clear();
            }
            Event::BpmEditToggled(current_bpm) => {
                if self.bpm_editing {
                    // 已在编辑状态 → 取消编辑
                    self.bpm_editing = false;
                    self.bpm_edit_buffer.clear();
                    self.bpm_edit_error = None;
                } else {
                    // 进入编辑状态，用当前 BPM 初始化缓冲区（整数不带 `.0`）
                    self.bpm_editing = true;
                    self.bpm_edit_buffer = lumino_core::shape::expr::fmt_number(current_bpm);
                    self.bpm_edit_error = None;
                    // 仪表盘同一时间只编辑一个字段
                    self.ppq_editing = false;
                    self.ppq_edit_buffer.clear();
                    self.ppq_edit_error = None;
                }
            }
            Event::BpmEditChanged(value) => {
                if self.bpm_editing {
                    // 只接受数字与小数点；负号 / 科学计数法直接丢弃。
                    if value.is_empty() || is_bpm_input(&value) {
                        self.bpm_edit_buffer = value;
                        self.bpm_edit_error = None;
                    }
                }
            }
            Event::BpmEditConfirmed => {
                self.bpm_editing = false;
                self.bpm_edit_buffer.clear();
            }
            Event::AutoScrollModeChanged => {
                self.auto_scroll_mode = match self.auto_scroll_mode {
                    lumino_core::storage::config::AutoScrollMode::FixedIndicatorLeft => {
                        lumino_core::storage::config::AutoScrollMode::ScrollingIndicator
                    }
                    lumino_core::storage::config::AutoScrollMode::ScrollingIndicator => {
                        lumino_core::storage::config::AutoScrollMode::Off
                    }
                    lumino_core::storage::config::AutoScrollMode::Off => {
                        lumino_core::storage::config::AutoScrollMode::FixedIndicatorLeft
                    }
                };
                tracing::debug!("工具栏: 自动滚动模式切换为 {:?}", self.auto_scroll_mode);
            }
            _ => {}
        }
    }
}

/// BPM 输入过滤：只允许 ASCII 数字与小数点。
///
/// 负号 / 指数 / 表达式字符直接丢弃，保留上一次合法输入；范围与语法在提交时
/// 由 `ToolbarHandler` 统一校验（与 PPQ 的「输入宽松、提交严格」同策略）。
fn is_bpm_input(value: &str) -> bool {
    value.chars().all(|c| c.is_ascii_digit() || c == '.')
}
