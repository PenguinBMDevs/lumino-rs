pub use crate::{
    settings::Event as Settings, sidebar::Event as Sidebar, toolbar::Event as Toolbar,
    window::Event as Window,
};

use crate::statusbar::performance::PerfData;

#[derive(Debug, Clone)]
pub enum EditorAction {
    Pressed {
        pos: iced_core::Point,
        shift: bool,
    },
    Moved(iced_core::Point),
    Released,
    Scrolled {
        delta_x: f32,
        delta_y: f32,
    },
    /// 双击事件
    DoubleClicked(iced_core::Point),
    /// 删除键按下（Delete 或 Backspace）
    DeletePressed,
    /// 剪切
    Cut,
    /// 复制
    Copy,
    /// 粘贴
    Paste,
    /// 全选
    SelectAll,
    /// 撤销
    Undo,
    /// 重做
    Redo,
    /// 标尺 scrubbing：设置播放位置（tick 值）
    Scrubbed {
        tick: f32,
    },
}

#[derive(Debug, Clone)]
pub enum AudioAction {
    PlayNote { key: u8, velocity: u8 },
    StopNote { key: u8 },
}

#[derive(Debug, Clone)]
pub enum Message {
    Core(lumino_core::Event),
    Window(Window),
    Sidebar(Sidebar),
    Progress(Option<(String, f64)>),
    ScrollbarScrolled(f32),  // 滚动条滚动事件，参数为新的scroll_x值
    ScrollbarScrolledY(f32), // 垂直滚动条滚动事件，参数为新的scroll_y值
    ZoomXChanged {
        zoom: f32,
        fixed_ratio: f32,
    }, // 横向缩放事件，参数为新的zoom_x值和固定点比例(0.0=左边缘, 1.0=右边缘)
    ZoomYChanged {
        zoom: f32,
        fixed_ratio: f32,
    }, // 纵向缩放事件，参数为新的zoom_y值和固定点比例(0.0=上边缘, 1.0=下边缘)
    /// Canvas 位置和尺寸更新，用于坐标转换和边界检测
    CanvasBoundsChanged {
        offset: iced_core::Point,
        size: iced_core::Size,
    },
    /// 音轨总览 Canvas 位置和尺寸更新
    ArrangementCanvasBoundsChanged {
        offset: iced_core::Point,
        size: iced_core::Size,
    },
    /// 菜单状态更新
    MenuStateChanged(bool), // true = 菜单打开，false = 菜单关闭
    EditorAction(EditorAction),
    AudioAction(AudioAction),
    /// 设置面板事件
    Settings(Settings),
    /// 切换设置面板显示状态
    ToggleSettings,
    /// 工具栏事件
    Toolbar(Toolbar),
    /// 打开自定义精度对话框
    OpenCustomPrecisionDialog,
    /// 关闭自定义精度对话框
    CloseCustomPrecisionDialog,
    /// 确认自定义精度
    ConfirmCustomPrecision,
    /// 三连音数量变更
    CustomPrecisionTupletCountChanged(String),
    /// 三连音类型变更
    CustomPrecisionTupletTypeChanged(crate::toolbar::TupletType),
    /// 符点类型变更
    CustomPrecisionDotTypeChanged(crate::toolbar::DotType),
    /// 分音符值变更
    CustomPrecisionNoteValueChanged(String),
    /// 除数变更
    CustomPrecisionDivisorChanged(String),
    /// 打开协作对话框
    OpenCollaborationDialog,
    /// 关闭协作对话框
    CloseCollaborationDialog,
    /// 连接协作服务器
    CollaborationConnect {
        host: String,
        port: u16,
        username: String,
        invite_code: Option<String>,
    },
    /// 创建协作房间
    CollaborationCreateRoom {
        name: String,
    },
    /// 加入协作房间
    CollaborationJoinRoom {
        invite_code: String,
    },
    /// 断开协作连接
    CollaborationDisconnect,
    /// 协作服务器地址变更
    CollaborationHostChanged(String),
    /// 协作服务器端口变更
    CollaborationPortChanged(String),
    /// 协作用户名变更
    CollaborationUsernameChanged(String),
    /// 协作房间名称变更
    CollaborationRoomNameChanged(String),
    /// 协作邀请码变更
    CollaborationInviteCodeChanged(String),
    /// 协作复制邀请码到剪贴板
    CollaborationCopyInviteCode,
    /// 协作远端鼠标移动
    CollaborationRemoteMouseMoved {
        user_id: std::sync::Arc<str>,
        x: f32,
        y: f32,
        color: std::sync::Arc<str>,
        username: std::sync::Arc<str>,
    },
    /// 协作用户离开
    CollaborationRemoteUserLeft {
        user_id: std::sync::Arc<str>,
    },
    /// 协作远端音符更新
    CollaborationRemoteNoteUpdate {
        operation: String,
    },
    /// 加载确认对话框 - 确认
    ConfirmLoadConfirm,
    /// 加载确认对话框 - 取消
    CloseLoadConfirmDialog,
    /// 打开工程设置对话框
    OpenProjectSettingsDialog,
    /// 关闭工程设置对话框
    CloseProjectSettingsDialog,
    /// 确认工程设置
    ConfirmProjectSettings,
    /// 工程设置 - 项目名称变更
    ProjectSettingsTitleChanged(String),
    /// 工程设置 - BPM 速度变更
    ProjectSettingsTempoChanged(String),
    /// 工程设置 - 版权信息变更
    ProjectSettingsCopyrightChanged(String),
    /// 打开设置对话框
    OpenSettingsDialog,
    /// 关闭设置对话框
    CloseSettingsDialog,
    /// 力度编辑面板动作
    Velocity(VelocityAction),
    /// 力度面板高度调整
    VelocityPanelResize(f32),
    /// 性能面板切换
    PerformancePanelToggled,
    /// 性能监控数据更新
    PerfUpdate(PerfData),
    /// 空消息标记
    Null,
    /// Ctrl 键状态变更
    CtrlKeyChanged(bool),
    ShiftKeyChanged(bool),
    /// 模式切换（编辑器/瀑布流）
    ModeToggled,
    /// 动画帧更新（用于弹簧物理模拟）
    AnimationTick,
    /// 循环区域事件
    LoopRange(LoopRangeAction),
    /// MIDI 输入事件（从 MIDI 设备收到的原始数据）
    MidiInputEvent {
        data: Vec<u8>,
    },
    /// 打开音频导出对话框
    OpenAudioExportDialog,
    /// 关闭音频导出对话框
    CloseAudioExportDialog,
    /// 确认音频导出
    AudioExportConfirm,
    /// 取消音频导出
    AudioExportCancel,
    /// 打开音符变速对话框
    OpenSpeedChangeDialog,
    /// 关闭音符变速对话框
    CloseSpeedChangeDialog,
    /// 确认音符变速
    ConfirmSpeedChange,
    /// 音符变速倍率输入变更
    SpeedChangeFactorChanged(String),
    /// 音频导出 - 工程名称变更
    AudioExportProjectNameChanged(String),
    /// 音频导出 - 输出格式变更
    AudioExportFormatChanged(crate::state::root_state::AudioFormat),
    /// 音频导出 - 采样率变更
    AudioExportSampleRateChanged(u32),
    /// 音频导出 - 通道数变更
    AudioExportChannelsChanged(crate::state::root_state::AudioChannels),
    /// 音频导出 - 层数限制变更
    AudioExportLayersChanged(String),
    /// 音频导出 - 通道多线程变更
    AudioExportChannelThreadingChanged(crate::state::root_state::ThreadingOption),
    /// 音频导出 - 按键多线程变更
    AudioExportKeyThreadingChanged(crate::state::root_state::ThreadingOption),
    /// 音频导出 - 插值算法变更
    AudioExportInterpolationChanged(crate::state::root_state::Interpolation),
    /// 音频导出 - 应用限制器变更
    AudioExportApplyLimiterChanged(bool),
    /// 音频导出 - 禁用淡出变更
    AudioExportDisableFadeOutChanged(bool),
    /// 音频导出 - 线性包络变更
    AudioExportLinearEnvelopeChanged(bool),
    /// 音频导出 - 输出路径变更
    AudioExportOutputPathChanged(String),
    /// 音频导出 - 浏览输出路径
    AudioExportBrowseOutput,
    /// 音频导出 - 进度更新
    AudioExportProgress(f32, String),
    /// 音频导出 - 完成
    AudioExportCompleted,
    /// 音频导出 - 失败
    AudioExportFailed(String),
    /// Pattern 编辑动作
    Pattern(PatternAction),
}

/// 循环区域动作
#[derive(Debug, Clone)]
pub enum LoopRangeAction {
    /// 切换循环启用/禁用
    Toggle,
    /// 设置循环范围（起始tick，结束tick）
    SetRange(f32, f32),
    /// 清除循环范围
    Clear,
    /// 标尺上鼠标按下（用于拖拽循环边界）
    RulerPressed { x: f32, y: f32 },
    /// 标尺上鼠标移动
    RulerMoved { x: f32, y: f32 },
    /// 标尺上鼠标释放
    RulerReleased,
    /// 标尺双击（切换循环）
    RulerDoubleClicked { x: f32, y: f32 },
}

/// 力度编辑面板动作
#[derive(Debug, Clone)]
pub enum VelocityAction {
    /// 拖拽开始：需要 push history 进行撤销支持
    /// 参数: (note_index, velocity)
    DragStart(usize, u8),
    /// 拖拽移动中：仅更新力度，不 push history
    /// 参数: (note_index, new_velocity)
    DragMove(usize, u8),
    /// 拖拽结束
    DragEnd,
    /// 曲线绘制开始：push history 保存绘制前状态
    CurveStart,
    /// 曲线绘制更新：批量应用力度变化，不 push history
    /// 参数: Vec<(note_index, new_velocity)>
    CurvePaint(Vec<(usize, u8)>),
    /// 曲线绘制结束
    CurveEnd,
}

/// Pattern 编辑动作（音轨总览中的音符片段）
#[derive(Debug, Clone)]
pub enum PatternAction {
    /// 选中 Pattern
    Selected(u32),
    /// 左边缘拖拽开始（参数: pattern_id）
    DragStartLeft(u32),
    /// 右边缘拖拽开始（参数: pattern_id）
    DragStartRight(u32),
    /// 左边缘拖拽移动中（参数: pattern_id, new_start_tick）
    DragMoveLeft(u32, f32),
    /// 右边缘拖拽移动中（参数: pattern_id, new_length）
    DragMoveRight(u32, f32),
    /// 拖拽结束
    DragEnd,
}

pub const fn null() -> Message {
    Message::Null
}
