//! 右侧栏面板内容模型（图片转 MIDI / 素材库 / 钢琴瀑布流）。
//!
//! 面板几何与生产一致：总宽 `panel_width` = 左侧 6px 拖拽手柄 + 内容区
//! `panel_width - 6`，右侧是固定 48px 图标列。拖拽手柄**不**进入占用列表，
//! 让 iced 继续处理缩放拖拽；内容区整块占用，点击不会再落到 iced 同名控件。
//!
//! 全部文案、工具提示行与配置文本由生产侧预计算，`lumino-ui-egui` 不做 i18n。

/// 右侧栏内容面板（互斥，同一时刻只有一个）。
#[derive(Debug, Clone, PartialEq)]
pub enum RightSidebarPanelModel {
    /// 图片转 MIDI。
    ImageToMidi(I2mPanelModel),
    /// 素材库。
    Materials(MaterialsPanelModel),
    /// 钢琴瀑布流预览（内容是外部离屏纹理）。
    PianoWaterfall(WaterfallPanelModel),
}

/// 钢琴瀑布流离屏纹理在桥接层的 egui 纹理 ID。
///
/// 宿主每帧把 `lumino-gfx` 渲染出的纹理视图注册到该 ID，模型只携带这个 ID；
/// 面板关闭时宿主注销，egui 侧不会持有 wgpu 资源。
pub const WATERFALL_TEXTURE_ID: egui::TextureId = egui::TextureId::User(0x4C55_4D49_4E46);

/// 编辑器画布（卷帘 / 走带）离屏纹理的 egui 外部纹理 ID。
///
/// 由 `lumino-gfx` 渲染线程产出、宿主每帧注册；egui 在最底层整窗采样它，
/// 取代原先「gfx 直接 blit 到 surface」的通道。
pub const EDITOR_TEXTURE_ID: egui::TextureId = egui::TextureId::User(0x4C55_4D49_4E45);

/// 钢琴瀑布流面板。
///
/// 瀑布流内容由 `lumino-gfx` 侧离屏渲染成 wgpu 纹理，宿主把它注册成 egui
/// 外部纹理后由本模型引用：`texture = None` 表示纹理尚未就绪（显示占位文案）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WaterfallPanelModel {
    /// 面板标题（「钢琴瀑布流预览」）。
    pub title: String,
    /// 纹理未就绪时的占位文案。
    pub empty_hint: String,
    /// 已注册的 egui 外部纹理。
    pub texture: Option<egui::TextureId>,
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

/// 素材库面板。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MaterialsPanelModel {
    /// 面板标题（「素材库」）。
    pub title: String,
    /// 「添加素材」按钮文本。
    pub add_label: String,
    /// 添加下拉菜单（`None` = 收起）。
    pub add_menu: Option<Vec<MaterialMenuEntryModel>>,
    /// 列表占位文本（未初始化 / 空列表）。
    pub placeholder: Option<String>,
    /// 重命名输入框占位符（「素材名称」）。
    pub rename_placeholder: String,
    /// 分区（内置素材 / 用户素材），空分区不出现。
    pub sections: Vec<MaterialSectionModel>,
    /// 右键菜单文案。
    pub context_labels: MaterialContextLabels,
    /// 右键上下文菜单（`None` = 关闭）。
    pub context_menu: Option<MaterialContextMenuModel>,
}

/// 右键菜单文案（生产侧按当前语言预计算）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MaterialContextLabels {
    /// 重命名。
    pub rename: String,
    /// 删除。
    pub delete: String,
    /// 上传到云。
    pub upload: String,
}

/// 添加下拉菜单项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialMenuEntryModel {
    /// 显示文本。
    pub label: String,
    /// 菜单项种类。
    pub kind: MaterialMenuKind,
}

/// 添加下拉菜单项种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialMenuKind {
    /// 从 web 下载。
    DownloadFromWeb,
    /// 从本地选取。
    ImportFromLocal,
}

/// 一个素材分区。
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialSectionModel {
    /// 分区标题（「内置素材」/「用户素材」）。
    pub label: String,
    /// 条目。
    pub entries: Vec<MaterialItemModel>,
}

/// 单个素材条目。
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialItemModel {
    /// 生产列表索引（动作回传）。
    pub index: usize,
    /// 显示名称。
    pub name: String,
    /// 是否有效（无效条目置灰且不可拖出）。
    pub valid: bool,
    /// 重命名缓冲（`Some` = 该条目处于重命名编辑态）。
    pub renaming: Option<String>,
    /// 悬停提示行（名称 / 作者 / 位置 / 轨道数 / 来源）。
    pub tooltip: Vec<String>,
}

/// 素材右键上下文菜单。
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialContextMenuModel {
    /// 目标条目索引。
    pub index: usize,
    /// 是否可编辑（内置素材全部置灰）。
    pub can_edit: bool,
    /// 面板局部坐标 x（快照）。
    pub x: f32,
    /// 面板局部坐标 y（快照）。
    pub y: f32,
}

/// 右键菜单项。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialContextMenuItem {
    /// 重命名。
    Rename,
    /// 删除。
    Delete,
    /// 上传到云。
    UploadToCloud,
}
