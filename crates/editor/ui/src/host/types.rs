//! Host 子模块 - 类型定义和工具函数

use std::path::PathBuf;

use iced_core::{Event, mouse, touch};

/// 音符数据: (tick, key, length, velocity, channel)
pub type NoteData = (f32, u8, f32, u8, u8);
/// 音轨音符数据: (track_idx, notes)
pub type TrackNotes = (usize, Vec<NoteData>);

/// 对话框结果
// Settings 变体已 Box 化（SettingsPanel 体积大，Box 降低 DialogResult 栈体积，
// 与 Message 枚举布局评审结论一致）。
#[derive(Debug, Clone)]
pub enum DialogResult {
    /// 自定义精度设置
    CustomPrecision {
        /// 分子
        numerator: String,
        /// 分母
        denominator: String,
    },
    /// 确认加载
    LoadConfirm,
    /// 取消操作
    Cancel,
    /// 保存确认对话框结果（关闭 / 打开 / 退出前的未保存更改确认）
    SaveConfirm(crate::message::SaveConfirmAction),
    /// 项目设置
    ProjectSettings {
        /// 项目标题
        title: String,
        /// 速度（BPM）
        tempo: f64,
        /// 版权信息
        copyright: String,
        /// 作者
        author: String,
        /// 拍号变化列表 (tick, 分子, 分母)
        time_signatures: Vec<(u32, u8, u8)>,
    },
    /// 设置结果
    Settings {
        /// 设置面板
        settings: Box<crate::settings::SettingsPanel>,
        /// 主题名称
        theme: String,
    },
    /// 速度变化结果
    SpeedChange {
        /// 变化倍率
        factor: f32,
    },
    /// 批量编辑结果
    BatchEdit {
        /// 力度
        velocity: String,
        /// 门限
        gate: String,
        /// 键位
        key: String,
        /// 节拍
        tick: String,
    },
    /// 找回删除音轨：恢复到原位置
    RecoverTrackRestore {
        /// 缓存文件路径（Runner 加载后写入 sidebar.tracks）
        path: PathBuf,
        /// 删除时记录的原始 sidebar.tracks 索引（恢复时优先放回此位置）
        original_index: usize,
    },
    /// 找回删除音轨：永久销毁
    RecoverTrackPermanentlyDelete {
        /// 缓存文件路径
        path: PathBuf,
        /// 删除时记录的音轨 ID（用于释放 reserved_track_ids 占用）
        track_id: u16,
    },
    /// 画刷「绘制行为」配置（Save 后由 runner 应用回主窗）
    BrushSettings(lumino_core::BrushConfig),
}

/// 将触摸事件转换为鼠标事件（兼容性处理）
///
/// 注意：只返回转换后的鼠标事件，不同时发送原始触摸事件，
/// 否则会导致单击被计为两次点击，触发错误的双击检测。
///
/// `FingerLost`（系统取消，如来电/切后台/手势被系统接管）必须**同时**保留
/// 原生事件并补一次左键释放：
/// - 原生事件让 iced 控件复位自己的按压态（`button.rs` 仅在 `FingerLost`
///   分支清 `is_pressed`）；
/// - 合成的 `ButtonReleased` 负责让画布/编辑器收尾——钢琴卷帘只认
///   `Mouse(ButtonReleased)`，不收尾会卡在绘制/拖动中间态。
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
            touch::Event::FingerLost { id, position } => {
                vec![
                    Event::Touch(touch::Event::FingerLost { id, position }),
                    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    Event::Mouse(mouse::Event::CursorMoved { position }),
                ]
            }
        },
        _ => vec![event],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_core::Point;

    /// 单指按下：合成「左键按下 + 光标定位」，顺序不能反
    /// （画布按下时用光标位置算命中，光标必须先到位）
    #[test]
    fn finger_pressed_becomes_button_press() {
        let events = convert_touch_to_mouse(Event::Touch(touch::Event::FingerPressed {
            id: touch::Finger(1),
            position: Point::new(12.0, 34.0),
        }));

        assert_eq!(
            events,
            vec![
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Mouse(mouse::Event::CursorMoved {
                    position: Point::new(12.0, 34.0),
                }),
            ]
        );
    }

    /// 单指抬起：合成「左键释放 + 光标定位」
    #[test]
    fn finger_lifted_becomes_button_release() {
        let events = convert_touch_to_mouse(Event::Touch(touch::Event::FingerLifted {
            id: touch::Finger(1),
            position: Point::new(12.0, 34.0),
        }));

        assert_eq!(
            events,
            vec![
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                Event::Mouse(mouse::Event::CursorMoved {
                    position: Point::new(12.0, 34.0),
                }),
            ]
        );
    }

    /// 单指移动：只合成光标移动，不产生按键事件
    #[test]
    fn finger_moved_only_moves_cursor() {
        let events = convert_touch_to_mouse(Event::Touch(touch::Event::FingerMoved {
            id: touch::Finger(1),
            position: Point::new(5.0, 6.0),
        }));

        assert_eq!(
            events,
            vec![Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(5.0, 6.0),
            })]
        );
    }

    /// 系统取消：必须保留原生 `FingerLost`（控件复位按压态）**并**补左键释放
    /// （画布/编辑器只认鼠标释放，不补会卡在绘制/拖动的中间态）
    #[test]
    fn finger_lost_keeps_native_event_and_releases_button() {
        let events = convert_touch_to_mouse(Event::Touch(touch::Event::FingerLost {
            id: touch::Finger(7),
            position: Point::new(1.0, 2.0),
        }));

        assert_eq!(
            events,
            vec![
                Event::Touch(touch::Event::FingerLost {
                    id: touch::Finger(7),
                    position: Point::new(1.0, 2.0),
                }),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                Event::Mouse(mouse::Event::CursorMoved {
                    position: Point::new(1.0, 2.0),
                }),
            ]
        );
    }

    /// 非触摸事件原样透传
    #[test]
    fn non_touch_event_passes_through() {
        let event = Event::Mouse(mouse::Event::CursorLeft);
        assert_eq!(convert_touch_to_mouse(event.clone()), vec![event]);
    }
}
