//! 右侧栏钢琴瀑布流预览面板
//!
//! 以「离屏 wgpu 渲染 → iced `shader` 图元直接合成」的方式绘制（GPU→GPU，零 CPU 读回）：
//! - 底部标准钢琴键盘（键数跟随全局 `enable_256key`：开启 256 键，否则 128 键）；
//! - 上方下落式音符瀑布流，复用渲染线程活体 GPU 实例缓冲（禁止第二份拷贝），
//!   配色与卷帘洋葱皮逐位一致，主音轨音符显示为蓝色，滚动与卷帘 X 缩放/滚动同步。
//!
//! 离屏纹理视图交给 iced `shader` 图元，在 iced 自身渲染通道内直接采样——
//! 与钢琴卷帘洋葱皮同一合成路径，因此不进 `image::Handle`、不进 iced 图集、不闪烁。

pub(crate) mod key_layout;
pub(crate) mod keyboard_renderer;
pub(crate) mod waterfall_primitive;

use std::sync::Arc;

use iced_core::{Length, Rectangle, mouse};
use iced_wgpu::wgpu;
use iced_widget::shader::{Program, Shader};
use iced_widget::{Column, container, text};

use lumino_extras::i18n::{Language, main_translations};

use crate::right_sidebar::core::{RESIZE_HANDLE_WIDTH, RightSidebar};
use crate::{Element, Message, Theme, window};

use self::keyboard_renderer::{KEY_HEIGHT_RATIO, MAX_KEY_HEIGHT, MIN_KEY_HEIGHT, PANEL_PADDING};
use self::waterfall_primitive::WaterfallPrimitive;

/// 瀑布流图元的 iced `shader` 程序：每帧把当前离屏纹理视图交给图元直接合成。
///
/// 右侧栏预览面板与全屏瀑布流播放器共用同一程序（样式完全一致）。
pub(crate) struct WaterfallProgram {
    /// 离屏纹理视图（`Arc` 克隆，纹理重建时旧视图仍可被在途图元安全引用）
    view: Arc<wgpu::TextureView>,
}

impl Program<Message> for WaterfallProgram {
    type State = ();
    type Primitive = WaterfallPrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        WaterfallPrimitive::new(self.view.clone())
    }
}

/// 渲染钢琴瀑布流预览面板内容（标题 + 说明 + 底部键盘图像）
pub(super) fn panel<'a>(
    right_sidebar: &'a RightSidebar,
    language: Language,
    _window: &'a window::Window,
) -> Element<'a> {
    let t = main_translations(language);
    let state = &right_sidebar.piano_waterfall;

    let top_col = Column::new()
        .spacing(8)
        .padding(PANEL_PADDING)
        .width(Length::Fill)
        .push(panel_header(format!("{}预览", t.piano_waterfall), _window));

    // 瀑布流占满面板剩余高度：用 iced `shader` 图元直接合成离屏纹理（GPU→GPU，不闪烁）
    let bottom_section: crate::Element<'a> = match &state.waterfall_view {
        Some(view) => Shader::new(WaterfallProgram { view: view.clone() })
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        None => text("（键盘渲染中或面板不可见）")
            .size(11)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.extended_palette().background.strong.text),
            })
            .into(),
    };

    let content_col = Column::new()
        .width(Length::Fill)
        .push(top_col)
        .push(bottom_section);

    let content = container(content_col)
        .width(Length::Fixed(
            right_sidebar.panel_width - RESIZE_HANDLE_WIDTH,
        ))
        .height(Length::Fill)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style::default().background(palette.background.weakest.color)
        });

    content.into()
}

/// 面板标题文本（跟随主题：暗色白、亮色黑）
fn panel_header<'a>(title: String, _window: &'a window::Window) -> Element<'a> {
    text(title)
        .size(14)
        .style(|theme: &Theme| text::Style {
            color: Some(theme.extended_palette().background.neutral.text),
        })
        .into()
}

/// 构造铺满容器的瀑布流 `shader` 图元（右侧栏预览与全屏播放器共用同款样式）。
///
/// 调用方负责用 `Length::Fill` 容器承载，使图元尺寸与离屏纹理尺寸一致（无拉伸）。
pub(crate) fn waterfall_shader_element<'a>(view: Arc<wgpu::TextureView>) -> Element<'a> {
    Shader::new(WaterfallProgram { view })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// 全屏瀑布流播放器状态：缓存离屏纹理视图、脏签名与布局尺寸。
///
/// 仅在 `AppMode::Waterfall` 下渲染；离开该模式即释放纹理（停止一切 GPU 渲染动作）。
pub(crate) struct WaterfallPlayerState {
    /// 离屏纹理视图（全屏瀑布流）
    pub view: Option<Arc<wgpu::TextureView>>,
    /// 脏判断签名（尺寸 / 键数 / 缩放 / 滚动 / 主音轨 / 音符数任一变化即重绘）
    pub cached_signature: Option<u64>,
    /// 主内容区实际像素尺寸（由 `responsive` 布局回调写入，供离屏渲染定尺寸）
    pub size: std::cell::RefCell<Option<(u32, u32)>>,
}

impl Default for WaterfallPlayerState {
    fn default() -> Self {
        Self {
            view: None,
            cached_signature: None,
            size: std::cell::RefCell::new(None),
        }
    }
}

/// 由面板内容宽度推导键盘渲染尺寸（宽度变化时高度按比例联动）
#[allow(dead_code)]
pub(crate) fn keyboard_size(content_width: f32) -> (u32, u32) {
    let w = (content_width - PANEL_PADDING * 2.0).max(1.0);
    let h = (w * KEY_HEIGHT_RATIO).clamp(MIN_KEY_HEIGHT, MAX_KEY_HEIGHT);
    (w as u32, h as u32)
}
