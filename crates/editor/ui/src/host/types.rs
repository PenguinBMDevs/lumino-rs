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
