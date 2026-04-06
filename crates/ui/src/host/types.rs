//! Host 子模块 - 类型定义和工具函数

use iced_core::{Event, mouse, touch};

/// 音符数据: (tick, key, length, velocity)
pub type NoteData = (f32, u8, f32, u8);
/// 音轨音符数据: (track_idx, notes)
pub type TrackNotes = (usize, Vec<NoteData>);

/// 对话框结果
#[derive(Debug, Clone)]
pub enum DialogResult {
    CustomPrecision {
        numerator: String,
        denominator: String,
    },
}

/// 将触摸事件转换为鼠标事件（兼容性处理）
///
/// 注意：只返回转换后的鼠标事件，不同时发送原始触摸事件，
/// 否则会导致单击被计为两次点击，触发错误的双击检测。
pub fn convert_touch_to_mouse(event: Event) -> Vec<Event> {
    match event {
        Event::Touch(touch_event) => match touch_event {
            touch::Event::FingerPressed { position, .. } => {
                vec![
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    Event::Mouse(mouse::Event::CursorMoved { position }),
                ]
            }
            touch::Event::FingerLifted { position, .. } => {
                vec![
                    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    Event::Mouse(mouse::Event::CursorMoved { position }),
                ]
            }
            touch::Event::FingerMoved { position, .. } => {
                vec![Event::Mouse(mouse::Event::CursorMoved { position })]
            }
            _ => vec![event],
        },
        _ => vec![event],
    }
}
