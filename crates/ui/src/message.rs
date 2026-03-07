pub use crate::{
    settings::Event as Settings, sidebar::Event as Sidebar, toolbar::Event as Toolbar,
    window::Event as Window,
};

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
    /// 自定义精度分子变更（已废弃）
    CustomPrecisionNumeratorChanged(String),
    /// 自定义精度分母变更（已废弃）
    CustomPrecisionDenominatorChanged(String),
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
    Null,
}

pub const fn null() -> Message {
    Message::Null
}
