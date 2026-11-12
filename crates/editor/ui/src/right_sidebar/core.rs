//! 右侧栏核心数据结构与常量

use std::sync::Arc;

use iced_wgpu::wgpu;

use crate::right_sidebar::i2m_cache::I2mCacheEntry;
use crate::right_sidebar::i2m_config::I2mConfig;
use crate::right_sidebar::material::MaterialLibrary;

/// 右侧栏图标栏宽度（固定，与左侧栏路由栏一致）
pub const ROUTE_BAR_WIDTH: f32 = 48.0;
/// 右侧栏面板默认宽度（与左侧栏面板一致）
pub const DEFAULT_PANEL_WIDTH: f32 = 200.0;
/// 右侧栏面板最小宽度
pub const MIN_PANEL_WIDTH: f32 = 150.0;
/// 右侧栏面板最大宽度
pub const MAX_PANEL_WIDTH: f32 = 900.0;
/// 右侧栏调整大小手柄宽度
pub const RESIZE_HANDLE_WIDTH: f32 = 6.0;

/// 右侧栏面板路由（互斥切换：同一时刻只显示一个面板）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RightSidebarPanel {
    /// 图片转 MIDI 面板
    #[default]
    ImageToMidi,
    /// 素材库面板
    Materials,
    /// 钢琴瀑布流预览面板
    PianoWaterfall,
}

/// 右侧栏状态
#[derive(Debug, Clone)]
pub struct RightSidebar {
    /// 面板是否可见
    pub panel_visible: bool,
    /// 当前激活的面板（I2M / 素材库，互斥切换）
    pub active_panel: RightSidebarPanel,
    /// 卷帘方向（由左侧栏底部「卷帘方向」按钮镜像，用于显隐面板入口）
    ///
    /// 纵向卷帘模式下，瀑布流预览窗面板入口按钮隐藏（其内容已并入纵向卷帘
    /// 编辑区，无需再经右侧栏入口打开）。默认横向。
    pub roll_mode: crate::sidebar::RollBarButton,
    /// 面板宽度
    pub panel_width: f32,
    /// 是否正在拖拽调整宽度
    pub is_resizing: bool,
    /// 拖拽开始时的鼠标 X 坐标
    pub resize_start_x: f32,
    /// 拖拽开始时的面板宽度
    pub resize_start_width: f32,
    /// 用户通过文件对话框选中的待转换图片路径
    pub selected_image_path: Option<std::path::PathBuf>,
    /// 是否正在后台执行图片转 MIDI 转换
    pub converting: bool,
    /// i2m 转换配置（用户自定义项）
    pub config: I2mConfig,
    /// 最近一次转换结果缓存：同一图片 + 同一参数时跳过解码/聚类/抖动
    pub i2m_cache: Option<I2mCacheEntry>,
    /// 素材库状态（素材列表 / 添加菜单）
    pub materials: MaterialLibrary,
    /// 钢琴瀑布流预览面板状态（含离屏 wgpu 键盘渲染产物）
    pub piano_waterfall: PianoWaterfallState,
}

/// 钢琴瀑布流预览面板状态
///
/// 持有离屏 wgpu 渲染得到的键盘纹理视图（`Arc<wgpu::TextureView>`），
/// 由 iced `shader` 图元在自身渲染通道内直接采样合成（GPU→GPU，无 CPU 读回、无
/// `image::Handle`、无图集重传），
/// 因此与钢琴卷帘洋葱皮同样不闪烁。仅用脏签名避免每帧重绘。
#[derive(Debug, Clone, Default)]
pub struct PianoWaterfallState {
    /// 当前键盘/瀑布流纹理视图；`None` 表示尚未渲染或面板不可见
    pub waterfall_view: Option<Arc<wgpu::TextureView>>,
    /// 上次渲染的参数签名（宽度/高度/键数等），用于脏判断
    pub cached_signature: Option<u64>,
    /// 缓存视图所属 GPU 代次（REND-019 PR-B：跨代次判废——失配时丢弃视图
    /// 与签名，走既有重渲染路径）
    pub cached_epoch: Option<lumino_gfx::GpuEpoch>,
}

impl RightSidebar {
    /// 创建一个默认的右侧栏
    pub fn new() -> Self {
        Self {
            panel_visible: false,
            active_panel: RightSidebarPanel::ImageToMidi,
            roll_mode: crate::sidebar::RollBarButton::Horizontal,
            panel_width: DEFAULT_PANEL_WIDTH,
            is_resizing: false,
            resize_start_x: 0.0,
            resize_start_width: DEFAULT_PANEL_WIDTH,
            selected_image_path: None,
            converting: false,
            config: I2mConfig::default(),
            i2m_cache: None,
            materials: MaterialLibrary::default(),
            piano_waterfall: PianoWaterfallState::default(),
        }
    }

    /// 计算右侧栏总宽度（图标栏 + 面板）
    pub fn width(&self) -> u32 {
        (ROUTE_BAR_WIDTH
            + if self.panel_visible {
                self.panel_width
            } else {
                0.0
            }) as u32
    }

    /// 切换面板路由并确保面板展开（互斥：仅显示目标面板）
    pub fn switch_panel(&mut self, panel: RightSidebarPanel) {
        self.active_panel = panel;
        self.panel_visible = true;
    }

    /// 当前按钮是否亮灯（面板可见且路由匹配）
    pub fn is_panel_active(&self, panel: RightSidebarPanel) -> bool {
        self.panel_visible && self.active_panel == panel
    }

    /// 设置选中的图片路径（并确保面板展开以便查看结果）
    pub fn set_selected_image_path(&mut self, path: std::path::PathBuf) {
        self.selected_image_path = Some(path);
        self.panel_visible = true;
    }
}

// 面板宽度拖拽调整（start_resize / update_resize_position / end_resize）
// 实现在 `resize` 子模块，按下手柄时由 Host 层以当前光标位置初始化锚点。

impl Default for RightSidebar {
    fn default() -> Self {
        Self::new()
    }
}
