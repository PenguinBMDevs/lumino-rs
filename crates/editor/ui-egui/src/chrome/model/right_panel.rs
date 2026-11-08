//! 右侧栏面板内容模型（当前接管「图片转 MIDI」）。
//!
//! 面板几何与生产一致：总宽 `panel_width` = 左侧 6px 拖拽手柄 + 内容区
//! `panel_width - 6`，右侧是固定 48px 图标列。拖拽手柄**不**进入占用列表，
//! 让 iced 继续处理缩放拖拽；内容区整块占用，点击不会再落到 iced 同名控件。
//!
//! 全部文案与配置文本由生产侧预计算，`lumino-ui-egui` 不做 i18n。

/// 右侧栏内容面板（互斥，同一时刻只有一个）。
#[derive(Debug, Clone, PartialEq)]
pub enum RightSidebarPanelModel {
    /// 图片转 MIDI。
    ImageToMidi(I2mPanelModel),
}

/// 图片转 MIDI 面板。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct I2mPanelModel {
    /// 面板标题（「图片转 MIDI」）。
    pub title: String,
    /// 「选择图片文件」按钮文本。
    pub select_image: String,
    /// 已选文件提示（`None` = 未选择）。
    pub selected_hint: Option<String>,
    /// 配置区小节标题（「转换参数」）。
    pub config_title: String,
    /// 配置行（自上而下）。
    pub rows: Vec<I2mRowModel>,
    /// 转换按钮（仅在已选择文件时出现；转换中禁用并改文案）。
    pub convert: Option<I2mConvertModel>,
}

/// 一行转换参数。
#[derive(Debug, Clone, PartialEq)]
pub struct I2mRowModel {
    /// 左侧标签。
    pub label: String,
    /// 右侧控件。
    pub control: I2mControlModel,
}

/// 转换参数控件。
#[derive(Debug, Clone, PartialEq)]
pub enum I2mControlModel {
    /// 单个数字输入框。
    Input {
        /// 字段。
        field: I2mField,
        /// 当前文本（生产输入缓冲）。
        text: String,
    },
    /// Key 范围：起止两个输入框 + 「~」。
    KeyRange {
        /// 起始字段。
        start_field: I2mField,
        /// 起始文本。
        start_text: String,
        /// 结束字段。
        end_field: I2mField,
        /// 结束文本。
        end_text: String,
    },
    /// 调色板算法下拉。
    Palette {
        /// 选项文本（已按当前语言翻译）。
        options: Vec<String>,
        /// 当前下标。
        selected: usize,
    },
}

/// 图片转 MIDI 可编辑字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I2mField {
    /// 最低 MIDI key。
    StartKey,
    /// 最高 MIDI key。
    EndKey,
    /// 目标高度。
    TargetHeight,
    /// 每像素 tick。
    TicksPerPixel,
    /// 颜色数。
    ColorCount,
}

/// 转换按钮状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct I2mConvertModel {
    /// 按钮文本（转换中为「转换中...」）。
    pub label: String,
    /// 是否可点（转换中禁用）。
    pub enabled: bool,
}
