//! Host 子模块 - 类型定义和工具函数

use std::path::PathBuf;

/// 音符数据: (tick, key, length, velocity, channel)
pub type NoteData = (f32, u8, f32, u8, u8);
/// 音轨音符数据: (track_idx, notes)
pub type TrackNotes = (usize, Vec<NoteData>);

/// 形状抽屉插入参数（随 DialogResult 跨窗口传递）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeInsertOptions {
    /// 插入宽度（拍）
    pub beats: f32,
    /// 插入高度（key）
    pub keys: f32,
    /// 保持原图比例（false = 拉伸铺满）
    pub fit_contain: bool,
}

impl Default for ShapeInsertOptions {
    fn default() -> Self {
        Self {
            beats: 4.0,
            keys: 12.0,
            fit_contain: false,
        }
    }
}

/// 对话框结果
// Settings 变体已 Box 化（SettingsPanel 体积大，Box 降低 DialogResult 栈体积，
// 与 Message 枚举布局评审结论一致）。
#[derive(Debug, Clone)]
pub enum DialogResult {
    /// 自定义吸附设置
    CustomPrecision(lumino_core::SnapSpec),
    /// 确认加载
    LoadConfirm,
    /// 取消操作
    Cancel,
    /// 保存确认对话框结果（关闭 / 打开 / 退出前的未保存更改确认）
    SaveConfirm(crate::message::SaveConfirmAction),
    /// 自动保存恢复对话框（启动时是否恢复上次会话的自动保存；DEBT-12）
    AutosaveRecover(crate::message::AutosaveRecoverAction),
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
        /// 启用通道 10（DRAW-032）
        use10: bool,
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
    /// 形状抽屉插入工程（抽屉保持打开）
    InsertShapeDrawing {
        /// 当前抽屉图形
        drawing: Box<lumino_core::shape::ShapeDrawing>,
        /// 插入尺寸/比例
        options: ShapeInsertOptions,
    },
    /// 历史面板请求跳到指定游标（历史窗口保持打开）
    HistoryJump(usize),
}
