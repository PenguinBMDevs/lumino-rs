//! 工具栏（含精度 / 洋葱皮 / 溢出菜单）的只读视图模型。
//!
//! 从 `chrome/model.rs` 拆出（REF-001：单文件 < 400 行）。

use lumino_core::SnapSpec;
use lumino_core::Tool;
use lumino_core::storage::config::AutoScrollMode;

/// 工具栏（72px band）的只读状态。
#[derive(Debug, Clone)]
pub struct ToolbarModel {
    /// 工具栏左边界（左侧栏占用宽度，逻辑点）。
    pub left_inset: f32,
    /// BPM 文本（保留 1 位小数）。
    pub bpm: String,
    /// PPQ 文本（形如 `PPQ 1920`）。
    pub ppq: String,
    /// 播放位置「小节.拍」。
    pub position: String,
    /// 播放时间 `mm:ss.cs`。
    pub time: String,
    /// 吸附规格文本（形如 `1/4`）。
    pub snap: String,
    /// 当前工具。
    pub tool: Tool,
    /// 绘制工具面板是否打开（第 5 个选择器的选中态）。
    pub draw_panel_open: bool,
    /// 是否正在播放（播放按钮显示暂停图标）。
    pub playing: bool,
    /// 是否循环播放（循环按钮图标切换）。
    pub looping: bool,
    /// 是否正在录制。
    pub recording: bool,
    /// 洋葱皮开关（`background.strong` 底色表示开启）。
    pub onion: bool,
    /// 自动滚动模式（决定右侧图标）。
    pub auto_scroll: AutoScrollMode,
    /// 精度下拉（`Some` = 由 egui 绘制；`None` = 关闭）。
    pub snap_menu: Option<SnapMenuModel>,
    /// 洋葱皮范围菜单（`Some` = 打开）。
    pub onion_menu: Option<OnionMenuModel>,
    /// 分组可见性（按真实折叠结果；全可见时与无溢出一致）。
    pub groups: ToolbarGroupsModel,
    /// 工具区可见条目数（`ALL` 的前缀长度；`usize::MAX` = 全部）。
    pub visible_tools: usize,
    /// 协作按钮是否可见（溢出时它最先被折叠）。
    pub collaboration_visible: bool,
    /// 「更多」按钮是否可见（有溢出才显示）。
    pub more_visible: bool,
    /// 溢出菜单内容（`Some` = 打开；生产折叠结果为空时始终 `None`）。
    pub overflow_menu: Option<OverflowMenuModel>,
    /// PPQ 是否处于编辑态（`true` 时仪表盘第一列画输入框）。
    pub ppq_editing: bool,
    /// PPQ 编辑缓冲文本。
    pub ppq_buffer: String,
    /// PPQ 编辑校验错误（非空时在输入框下方显示）。
    pub ppq_error: Option<String>,
}

/// 精度（吸附）下拉的内容。
#[derive(Debug, Clone, Default)]
pub struct SnapMenuModel {
    /// 预设项（与 `SnapSpec::presets()` 同序）。
    pub items: Vec<SnapMenuItem>,
    /// 自定义项标签（生产固定为 `Custom…`）。
    pub custom_label: String,
}

/// 单个精度预设。
#[derive(Debug, Clone)]
pub struct SnapMenuItem {
    /// 吸附规格（点击后原样回传生产）。
    pub spec: SnapSpec,
    /// 显示文本（`SnapSpec::text()`）。
    pub label: String,
    /// 是否为当前规格。
    pub selected: bool,
}

/// 洋葱皮「显示范围」菜单。
#[derive(Debug, Clone, Default)]
pub struct OnionMenuModel {
    /// 四个范围（顺序 = `OnionSkinMode::all()`）。
    pub items: Vec<OnionMenuItem>,
}

/// 单个洋葱皮范围项。
#[derive(Debug, Clone)]
pub struct OnionMenuItem {
    /// 本地化显示名。
    pub label: String,
    /// 是否为当前模式。
    pub selected: bool,
}

/// 工具栏分组可见性（宽度不足时生产按优先级折叠）。
///
/// 默认全部可见：与「无溢出」的视觉一致；生产模型会按真实折叠结果覆盖。
#[derive(Debug, Clone, Copy)]
pub struct ToolbarGroupsModel {
    /// 录制按钮组。
    pub record: bool,
    /// 播放控制组。
    pub playback: bool,
    /// 循环按钮。
    pub looping: bool,
    /// 撤销/重做/历史。
    pub undo_redo: bool,
    /// 检测仪表盘。
    pub dashboard: bool,
    /// 工具选择区（含精度选择器）。
    pub tools: bool,
    /// 洋葱皮开关。
    pub onion: bool,
    /// 自动滚动。
    pub auto_scroll: bool,
}

impl Default for ToolbarGroupsModel {
    fn default() -> Self {
        Self {
            record: true,
            playback: true,
            looping: true,
            undo_redo: true,
            dashboard: true,
            tools: true,
            onion: true,
            auto_scroll: true,
        }
    }
}

/// 「更多」溢出菜单：折叠掉的工具以网格按钮补偿。
#[derive(Debug, Clone, Default)]
pub struct OverflowMenuModel {
    /// 菜单项（顺序 = 生产 `overflow_menu_items`）。
    pub items: Vec<OverflowItemModel>,
}

/// 单个溢出菜单按钮。
#[derive(Debug, Clone)]
pub struct OverflowItemModel {
    /// 图标（与生产 `resources::icon::Icon` 一一映射）。
    pub icon: crate::icons::Icon,
    /// 悬浮提示（生产 `tooltip`）。
    pub tooltip: String,
    /// 是否可用（依赖选中的动作在无选中时置灰）。
    pub enabled: bool,
}

impl Default for ToolbarModel {
    fn default() -> Self {
        Self {
            left_inset: 0.0,
            bpm: "120.0".to_owned(),
            ppq: "PPQ 1920".to_owned(),
            position: "1.1".to_owned(),
            time: "00:00.00".to_owned(),
            snap: "1/4".to_owned(),
            tool: Tool::Pointer,
            draw_panel_open: false,
            playing: false,
            looping: false,
            recording: false,
            onion: false,
            auto_scroll: AutoScrollMode::ScrollingIndicator,
            snap_menu: None,
            onion_menu: None,
            groups: ToolbarGroupsModel::default(),
            visible_tools: usize::MAX,
            collaboration_visible: false,
            more_visible: true,
            overflow_menu: None,
            ppq_editing: false,
            ppq_buffer: String::new(),
            ppq_error: None,
        }
    }
}
