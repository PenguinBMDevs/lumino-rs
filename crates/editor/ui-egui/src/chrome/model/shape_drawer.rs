//! 形状抽屉左侧面板模型（画布仍由 iced 通道绘制）。

/// 形状抽屉面板：库列表 / 文件与编辑动作 / 面积填充 / 插入与缩放。
#[derive(Debug, Clone)]
pub struct ShapeDrawerModel {
    /// 顶部是否有 30px iced 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 面板矩形（逻辑坐标；宽 = `panel_width`，高 = 标题栏以下整块）。
    pub rect: egui::Rect,
    /// 面板底色（跟随主题 `background.base`）。
    pub title: String,
    /// 库条目。
    pub rows: Vec<ShapeRowModel>,
    /// 名称输入。
    pub name_input: String,
    /// 名称占位（「Shape name」）。
    pub name_hint: String,
    /// 文件动作按钮（New / Save / Rename / Del）。
    pub file_actions: Vec<ShapeActionButton>,
    /// 编辑动作按钮（Copy / Paste / FlipH / FlipV / Rot90）。
    pub edit_actions: Vec<ShapeActionButton>,
    /// 面积模式开关文案（「Area: on」/「Area」）。
    pub area_toggle_label: String,
    /// 面积模式是否开启（决定按钮高亮）。
    pub area_mode: bool,
    /// 「Clear」。
    pub area_clear: String,
    /// 当前面积色说明（`current colour: 3` / `single closed stroke only`）。
    pub area_hint: String,
    /// 16 个色块（文案为 0..15）。
    pub swatches: Vec<ShapeSwatchModel>,
    /// 状态 / 错误提示（11px）。
    pub status: Option<String>,
    /// 库加载 warning（10px，自动换行）。
    pub warning: Option<String>,
    /// 「Size」标签。
    pub size_label: String,
    /// 插入宽度（拍）。
    pub insert_beats: String,
    /// 插入宽度占位。
    pub insert_beats_hint: String,
    /// 「x」分隔。
    pub times_label: String,
    /// 插入高度（key）。
    pub insert_keys: String,
    /// 插入高度占位。
    pub insert_keys_hint: String,
    /// 保持比例开关（「Contain」/「Stretch」）。
    pub fit_label: String,
    /// 是否保持比例。
    pub fit_contain: bool,
    /// 「Scale」标签。
    pub scale_label: String,
    /// 缩放百分比文本。
    pub scale_percent: String,
    /// 缩放占位（「%」）。
    pub scale_hint: String,
    /// 「Apply」。
    pub scale_apply: String,
    /// 锚点弯折按钮（`None` = 当前不是曲线锚点）。
    pub bend: Option<String>,
    /// 锚点是否为 Corner（true = 直角）。
    pub bend_corner: bool,
    /// 「Insert」按钮。
    pub insert: String,
}

/// 库条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeRowModel {
    /// 图形 id（动作回传）。
    pub id: String,
    /// 显示名（选中时前缀「● 」）。
    pub label: String,
    /// 是否选中。
    pub selected: bool,
}

/// 面板里的一个动作按钮。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeActionButton {
    /// 文案。
    pub label: String,
    /// 对应动作。
    pub action: ShapePanelAction,
}

/// 面积色块。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeSwatchModel {
    /// 颜色下标（0..=15）。
    pub colour: u8,
    /// 文案（下标字符串）。
    pub label: String,
    /// 是否为当前面积色。
    pub selected: bool,
}

/// 面板动作（由生产侧翻译成 `Message`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapePanelAction {
    /// 选中库图形。
    Select(String),
    /// 新建。
    New,
    /// 保存。
    Save,
    /// 重命名。
    Rename,
    /// 删除。
    Delete,
    /// 复制 strokes。
    Copy,
    /// 粘贴 strokes。
    Paste,
    /// 水平翻转。
    FlipHorizontal,
    /// 垂直翻转。
    FlipVertical,
    /// 旋转 90°。
    Rotate90,
    /// 开合面积模式。
    ToggleAreaMode,
    /// 清空面积。
    ClearAreas,
    /// 设置面积色。
    SetAreaColour(u8),
    /// 名称输入变化。
    SetName(String),
    /// 插入宽度变化。
    SetInsertBeats(String),
    /// 插入高度变化。
    SetInsertKeys(String),
    /// 开合保持比例。
    ToggleInsertFit,
    /// 缩放百分比变化。
    SetScalePercent(String),
    /// 应用缩放。
    ApplyScale,
    /// 开合锚点弯折。
    ToggleAnchorBend,
    /// 插入到工程。
    Insert,
}

impl Default for ShapeDrawerModel {
    fn default() -> Self {
        Self {
            titlebar: false,
            rect: egui::Rect::NOTHING,
            title: String::new(),
            rows: Vec::new(),
            name_input: String::new(),
            name_hint: String::new(),
            file_actions: Vec::new(),
            edit_actions: Vec::new(),
            area_toggle_label: String::new(),
            area_mode: false,
            area_clear: String::new(),
            area_hint: String::new(),
            swatches: Vec::new(),
            status: None,
            warning: None,
            size_label: String::new(),
            insert_beats: String::new(),
            insert_beats_hint: String::new(),
            times_label: String::new(),
            insert_keys: String::new(),
            insert_keys_hint: String::new(),
            fit_label: String::new(),
            fit_contain: false,
            scale_label: String::new(),
            scale_percent: String::new(),
            scale_hint: String::new(),
            scale_apply: String::new(),
            bend: None,
            bend_corner: false,
            insert: String::new(),
        }
    }
}
