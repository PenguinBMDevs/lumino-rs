//! 视频导出面板模型（整块替换右侧内容区，形态同音频导出）。
//!
//! 控件用通用行描述（下拉 / 滑杆 / 复选 / 文本 / 标签 / 按钮），生产侧只负责把
//! 状态翻译成行并用 `id` 记住回传目标，渲染层不认识任何业务枚举。

/// 视频导出面板（含占用矩形）。
#[derive(Debug, Clone)]
pub struct VideoExportPanelModel {
    /// 面板占据的窗口区域（逻辑坐标）。
    pub rect: egui::Rect,
    /// 面板内容。
    pub content: VideoExportModel,
}

impl Default for VideoExportPanelModel {
    fn default() -> Self {
        Self {
            rect: egui::Rect::NOTHING,
            content: VideoExportModel::default(),
        }
    }
}

/// 面板内容。
#[derive(Debug, Clone, Default)]
pub struct VideoExportModel {
    /// 大标题（18px）。
    pub title: String,
    /// 分区（自上而下）。
    pub sections: Vec<VideoSection>,
    /// 预览区（空态文案；`None` = 不显示）。
    pub preview: Option<VideoPreviewModel>,
    /// 底部按钮。
    pub buttons: Vec<VideoButton>,
}

/// 一个分区。
#[derive(Debug, Clone, Default)]
pub struct VideoSection {
    /// 小标题（16px；空串 = 无标题）。
    pub title: String,
    /// 分区内的行。
    pub rows: Vec<VideoRow>,
}

/// 预览区（空态）。
#[derive(Debug, Clone, Default)]
pub struct VideoPreviewModel {
    /// 空态文案（「等待渲染...」）。
    pub label: String,
    /// 有缓存图时的显示尺寸（逻辑像素）；`None` = 暂无图。
    pub image_size: Option<(f32, f32)>,
}

/// 一行控件。
#[derive(Debug, Clone)]
pub enum VideoRow {
    /// 下拉选择。
    Pick {
        /// 标签。
        label: String,
        /// 标签列宽（生产 100 / 50 / 70）。
        label_width: f32,
        /// 选项（下标即回传值）。
        options: Vec<String>,
        /// 当前选项下标。
        selected: usize,
        /// 控件宽（生产多为 200）。
        width: f32,
        /// 回传 id。
        id: u16,
    },
    /// 滑杆（含右侧数值文本）。
    Slider {
        /// 标签。
        label: String,
        /// 标签列宽。
        label_width: f32,
        /// 当前值。
        value: f32,
        /// 最小值。
        min: f32,
        /// 最大值。
        max: f32,
        /// 步长。
        step: f32,
        /// 滑杆宽（生产 200 / 140）。
        slider_width: f32,
        /// 右侧数值格式（如 `1.0x` / `7.5` / `24 px`）。
        value_text: String,
        /// 回传 id。
        id: u16,
    },
    /// 复选框。
    Toggle {
        /// 标签。
        label: String,
        /// 是否勾选。
        checked: bool,
        /// 回传 id。
        id: u16,
    },
    /// 文本框。
    Text {
        /// 标签（空串 = 无标签，占满整行）。
        label: String,
        /// 标签列宽。
        label_width: f32,
        /// 当前值。
        value: String,
        /// 占位。
        hint: String,
        /// 控件宽（0 = 填满剩余）。
        width: f32,
        /// 回传 id。
        id: u16,
    },
    /// 多行文本域（模板编辑器）。
    TextArea {
        /// 标签。
        label: String,
        /// 当前值。
        value: String,
        /// 占位。
        hint: String,
        /// 高度。
        height: f32,
        /// 回传 id。
        id: u16,
    },
    /// 「标签 + 输入 + 行尾按钮」（路径 + 浏览）。
    TextButton {
        /// 标签。
        label: String,
        /// 标签列宽。
        label_width: f32,
        /// 当前值。
        value: String,
        /// 占位。
        hint: String,
        /// 输入宽（0 = 填满到按钮左侧）。
        width: f32,
        /// 输入回传 id。
        text_id: u16,
        /// 行尾按钮（自带 id）。
        button: VideoButton,
    },
    /// 一行两个「标签 + 输入」（生产的分辨率 / 数值对）。
    TextPair {
        /// 左列。
        left: TextPairCell,
        /// 右列。
        right: TextPairCell,
        /// 两列之间的分隔（如「x」；空串 = 仅间距）。
        separator: String,
    },
    /// 显式竖向间距（生产用 `space().height(n)` 分隔子区块）。
    Gap {
        /// 高度。
        height: f32,
    },
    /// 只读文本行（说明 / 提示）。
    Label {
        /// 内容。
        text: String,
        /// 字号。
        size: f32,
        /// 是否弱化色。
        muted: bool,
    },
    /// 一行按钮（如「恢复默认」「完整模板」）。
    Buttons {
        /// 按钮。
        buttons: Vec<VideoButton>,
    },
}

/// 面板按钮。
#[derive(Debug, Clone, PartialEq)]
pub struct VideoButton {
    /// 文案。
    pub label: String,
    /// 回传 id。
    pub id: u16,
    /// 底色。
    pub fill: VideoButtonFill,
    /// 宽度（0 = 随文本）。
    pub width: f32,
}

/// 按钮底色语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoButtonFill {
    /// 主色。
    Primary,
    /// 次要（弱底）。
    Neutral,
    /// 危险。
    Danger,
}

/// 面板产生的动作。
#[derive(Debug, Clone, PartialEq)]
pub enum VideoExportAction {
    /// 下拉选择（`id` + 选项下标）。
    Pick {
        /// 回传 id。
        id: u16,
        /// 选项下标。
        index: usize,
    },
    /// 滑杆值。
    Slider {
        /// 回传 id。
        id: u16,
        /// 新值。
        value: f32,
    },
    /// 复选切换。
    Toggle {
        /// 回传 id。
        id: u16,
        /// 新值。
        value: bool,
    },
    /// 文本变化。
    Text {
        /// 回传 id。
        id: u16,
        /// 新文本。
        value: String,
    },
    /// 按钮点击。
    Button {
        /// 回传 id。
        id: u16,
    },
}

/// 数值对中的一列。
#[derive(Debug, Clone, Default)]
pub struct TextPairCell {
    /// 标签（空串 = 无标签）。
    pub label: String,
    /// 标签列宽。
    pub label_width: f32,
    /// 当前值。
    pub value: String,
    /// 占位。
    pub hint: String,
    /// 输入宽。
    pub width: f32,
    /// 回传 id。
    pub id: u16,
}
