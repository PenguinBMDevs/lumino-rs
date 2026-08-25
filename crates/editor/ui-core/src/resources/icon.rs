use iced_core::Color;
use iced_widget::svg::{Handle as SvgHandle, Svg};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// 当前 HiDPI 图标渲染状态（兼容旧设置项，SVG 矢量直渲已无需 2x 位图，保留仅为触发缓存刷新）
static HIDPI_ENABLED: AtomicBool = AtomicBool::new(true);

/// 缓存的 SVG Handle 对象，key=Icon，避免每帧创建新 Handle
static SVG_HANDLE_CACHE: Lazy<Mutex<HashMap<Icon, SvgHandle>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub use Icon::*;

/// 图标加载错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum IconError {
    /// 图标不在缓存中（至少需先加载一次）
    #[error("图标 {0:?} 不在缓存中")]
    IconNotInCache(Icon),
    /// SVG 解析错误
    #[error("SVG 解析错误: {0}")]
    SvgParseError(String),
    /// 无法创建 pixmap
    #[error("无法创建 pixmap")]
    PixmapCreationError,
    /// 获取缓存锁失败
    #[error("获取缓存锁失败")]
    LockError,
}

// ─── 图标定义宏：一处定义 → 三处生成（枚举 + 全部枚举数组 + bytes 匹配） ───
macro_rules! define_icons {
    ($(($name:ident, $path:expr)),* $(,)?) => {
        /// 图标变体枚举（由 define_icons! 宏生成）
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Icon {
            $(/// 图标变体 `Icon::$name`
            $name,)*
        }

        /// 全部图标枚举（与宏定义同源，供测试/遍历使用）
        pub const ALL_ICONS: &[Icon] = &[$($name,)*];

        fn bytes(icon: Icon) -> &'static [u8] {
            match icon {
                $(Icon::$name => include_bytes!($path),)*
            }
        }
    };
}

define_icons! {
    (AngleRight, "../../../../../resources/icons/regular/submenu-expand-indicator.svg"),
    (FolderTree, "../../../../../resources/icons/regular/ui-layout.svg"),
    (Arrangement, "../../../../../resources/icons/regular/arrangement-mode.svg"),
    (Gear, "../../../../../resources/icons/regular/settings-general.svg"),
    (WaveForm, "../../../../../resources/icons/regular/audio-automation.svg"),
    (Lumino, "../../../../../resources/icons/brands/lumino-brand.svg"),
    (LogoInApp, "../../../../../resources/icons/brands/app-logo.svg"),
    (WindowMin, "../../../../../resources/icons/window/min.svg"),
    (WindowMax, "../../../../../resources/icons/window/max.svg"),
    (WindowUnMax, "../../../../../resources/icons/window/unmax.svg"),
    (WindowClose, "../../../../../resources/icons/window/close.svg"),
    (Clock, "../../../../../resources/icons/sidebar/conductor-track.svg"),
    (Eye, "../../../../../resources/icons/sidebar/onion-skin.svg"),
    (EyeSlash, "../../../../../resources/icons/sidebar/onion-skin-disabled.svg"),
    (Plus, "../../../../../resources/icons/sidebar/add-track.svg"),
    (Download, "../../../../../resources/icons/sidebar/export-group.svg"),
    (PlayCircle, "../../../../../resources/icons/sidebar/waterfall-record.svg"),
    (EllipsisVertical, "../../../../../resources/icons/sidebar/toolbar-overflow-trigger.svg"),
    // 卷帘面板左侧栏底部按钮（横向 / 纵向三条杠）
    (RollBarHorizontal, "../../../../../resources/icons/sidebar/roll-bar-horizontal.svg"),
    (RollBarVertical, "../../../../../resources/icons/sidebar/roll-bar-vertical.svg"),
    (Users, "../../../../../resources/icons/toolbar/collaboration.svg"),
    // 工具栏图标
    (Play, "../../../../../resources/icons/toolbar/playback-start.svg"),
    (Pause, "../../../../../resources/icons/toolbar/playback-pause.svg"),
    (SkipBackward, "../../../../../resources/icons/toolbar/playback-jump-start.svg"),
    (SkipForward, "../../../../../resources/icons/toolbar/playback-jump-end.svg"),
    (Undo, "../../../../../resources/icons/toolbar/history-undo.svg"),
    (Redo, "../../../../../resources/icons/toolbar/history-redo.svg"),
    (MousePointer, "../../../../../resources/icons/toolbar/select-tool.svg"),
    (MousePointerYSelect, "../../../../../resources/icons/toolbar/y-axis-select-tool.svg"),
    (Pencil, "../../../../../resources/icons/toolbar/note-draw-tool.svg"),
    (Eraser, "../../../../../resources/icons/toolbar/eraser-tool.svg"),
    (Curve, "../../../../../resources/icons/toolbar/curve-tool.svg"),
    (PaintBucket, "../../../../../resources/icons/toolbar/paint-bucket.svg"),
    (Quantize, "../../../../../resources/icons/toolbar/note-quantize.svg"),
    (Speed, "../../../../../resources/icons/toolbar/playback-speed.svg"),
    // 工具面板图标（SVG 矢量直渲迁移时遗漏，补齐以兼容 dev 侧引用）
    (BrushTool, "../../../../../resources/icons/toolbar/brush-tool.svg"),
    (ShapeTool, "../../../../../resources/icons/toolbar/shape-tool.svg"),
    (TextInput, "../../../../../resources/icons/toolbar/text-input.svg"),
    (ToolPanelCaret, "../../../../../resources/icons/toolbar/caret-down.svg"),
    (PlusCircle, "../../../../../resources/icons/toolbar/plus-circle.svg"),
    (MinusCircle, "../../../../../resources/icons/toolbar/minus-circle.svg"),
    // 音符翻转图标
    (FlipVertical, "../../../../../resources/icons/toolbar/note-flip-vertical.svg"),
    (FlipHorizontal, "../../../../../resources/icons/toolbar/note-flip-horizontal.svg"),
    // 自动滚动图标
    (ArrowsLeftRight, "../../../../../resources/icons/toolbar/loop-range-active.svg"),
    (Scroll, "../../../../../resources/icons/toolbar/autoscroll-scrolling.svg"),
    (Ban, "../../../../../resources/icons/toolbar/loop-range-disabled.svg"),
    // 移调/分割/合并 图标
    (TransposeUp, "../../../../../resources/icons/toolbar/note-transpose-up.svg"),
    (TransposeDown, "../../../../../resources/icons/toolbar/note-transpose-down.svg"),
    (Split, "../../../../../resources/icons/toolbar/note-split.svg"),
    (Glue, "../../../../../resources/icons/toolbar/note-glue.svg"),
    // 连奏/同音连接
    (Tie, "../../../../../resources/icons/toolbar/note-tie.svg"),
    // 图片转 MIDI 占位图标
    (ImageToMidi, "../../../../../resources/icons/toolbar/image-to-midi-converter.svg"),
    // 素材库图标（右侧栏）
    (MaterialLibrary, "../../../../../resources/icons/toolbar/material-library.svg"),
    // 钢琴瀑布流预览图标（右侧栏）
    (PianoWaterfall, "../../../../../resources/icons/sidebar/piano-waterfall.svg"),
    // 标题栏图标
    (PencilOutline, "../../../../../resources/icons/titlebar/editor-mode.svg"),
    (Keys, "../../../../../resources/icons/titlebar/piano-roll.svg"),
    (VideoCamera, "../../../../../resources/icons/sidebar/video-export.svg"),
    (MusicNote, "../../../../../resources/icons/sidebar/audio-export.svg"),
    // 钢琴卷帘右键上下文菜单图标
    (ContextMenuCut, "../../../../../resources/icons/context-menu/cut-notes.svg"),
    (ContextMenuCopy, "../../../../../resources/icons/context-menu/copy-notes.svg"),
    (ContextMenuPaste, "../../../../../resources/icons/context-menu/paste-notes.svg"),
    (ContextMenuDelete, "../../../../../resources/icons/context-menu/delete-item.svg"),
    (ContextMenuSelectAll, "../../../../../resources/icons/context-menu/select-all-notes.svg"),
    (ContextMenuColorPalette, "../../../../../resources/icons/context-menu/set-track-color.svg"),
    (ContextMenuChannel, "../../../../../resources/icons/context-menu/set-midi-channel.svg"),
    (ContextMenuRecoverTrack, "../../../../../resources/icons/context-menu/recover-deleted-track.svg"),
    // 素材库右键菜单图标
    (ContextMenuUploadToCloud, "../../../../../resources/icons/context-menu/upload-to-cloud.svg"),
    (Mixer, "../../../../../resources/icons/sidebar/mixer.svg"),
    (MixerActive, "../../../../../resources/icons/sidebar/mixer-active.svg"),
}

#[derive(Clone)]
struct IconData {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

/// 设置 HiDPI 状态并刷新 SVG 缓存（SVG 矢量无需重光栅，清空缓存仅为兼容旧逻辑触发界面重绘）
pub fn set_hidpi_enabled(enabled: bool) {
    HIDPI_ENABLED.store(enabled, Ordering::Relaxed);
    if let Ok(mut cache) = SVG_HANDLE_CACHE.lock() {
        cache.clear();
    }
}

/// 获取或创建缓存的 SVG Handle。
fn get_or_create_svg_handle(icon: Icon) -> Result<SvgHandle, IconError> {
    let mut cache = SVG_HANDLE_CACHE.lock().map_err(|_| IconError::LockError)?;
    if let Some(handle) = cache.get(&icon) {
        return Ok(handle.clone());
    }
    let handle = SvgHandle::from_memory(bytes(icon));
    cache.insert(icon, handle.clone());
    Ok(handle)
}

/// 判断指定图标在当前主题下是否需要反色。
/// Logo 类图标（Lumino / LogoInApp）在暗色/亮色模式下均保持原色，不反色。
fn should_invert_icon(icon: Icon, is_dark: bool) -> bool {
    // MixerActive 为「亮灯」态固定琥珀色，不参与反色，确保打开面板时图标常亮。
    is_dark && !matches!(icon, Icon::Lumino | Icon::LogoInApp | Icon::MixerActive)
}

fn is_dark_theme(theme: Option<&crate::Theme>) -> bool {
    if crate::theme::is_high_contrast() {
        true
    } else {
        theme
            .map(|t| t.extended_palette().background.weakest.color.r < 0.5)
            .unwrap_or(true)
    }
}

fn svg_element(icon: Icon, width: u32, height: u32, is_dark: bool) -> Svg<'static, crate::Theme> {
    // 兼容性：先从缓存取 Handle，失败则直接 from_memory（理论上不失败）
    let handle = get_or_create_svg_handle(icon)
        .unwrap_or_else(|_| SvgHandle::from_memory(bytes(icon)));
    let mut svg = Svg::new(handle)
        .width(iced_core::Length::Fixed(width as f32))
        .height(iced_core::Length::Fixed(height as f32));
    if should_invert_icon(icon, is_dark) {
        // 整体染色为白色，覆盖 SVG 内硬编码的 #000000，保持与旧 invert_rgba 行为一致
        svg = svg.style(|_theme, _status| iced_widget::svg::Style {
            color: Some(Color::WHITE),
        });
    }
    svg
}

/// 渲染图标（可能 panic，仅用于向后兼容）
pub fn view(icon: Icon) -> crate::Element<'static> {
    match view_safe(icon) {
        Ok(element) => element,
        Err(e) => {
            tracing::error!("渲染图标失败: {}", e);
            iced_widget::Space::new()
                .width(iced_core::Length::Fixed(24.0))
                .height(iced_core::Length::Fixed(24.0))
                .into()
        }
    }
}

/// 安全地渲染图标，返回 Result
pub fn view_safe(icon: Icon) -> Result<crate::Element<'static>, IconError> {
    // 旧逻辑硬编码为浅色（is_dark=false → 黑色），保持兼容
    let element: crate::Element<'static> = svg_element(icon, 24, 24, false).into();
    Ok(element)
}

/// 渲染指定尺寸和主题的图标（可能 panic，仅用于向后兼容）
pub fn view_with_size_and_theme(
    icon: Icon,
    width: u32,
    height: u32,
    theme: Option<&crate::Theme>,
) -> crate::Element<'static> {
    match view_with_size_and_theme_safe(icon, width, height, theme) {
        Ok(element) => element,
        Err(e) => {
            tracing::error!("渲染图标失败: {}", e);
            iced_widget::Space::new()
                .width(iced_core::Length::Fixed(width as f32))
                .height(iced_core::Length::Fixed(height as f32))
                .into()
        }
    }
}

/// 将任意 SVG 数据光栅化为 iced 图像句柄（供 canvas 等非 widget 场景复用）
///
/// 复用 `usvg + resvg` 渲染管线，与内置图标一致；尺寸为正方形画布。
pub fn svg_handle(svg_data: &[u8], size: u32) -> Result<iced_core::image::Handle, IconError> {
    let data = render_svg(svg_data, size, size)?;
    Ok(iced_core::image::Handle::from_rgba(
        data.width,
        data.height,
        data.rgba,
    ))
}

/// 安全地渲染指定尺寸和主题的图标，返回 Result
pub fn view_with_size_and_theme_safe(
    icon: Icon,
    width: u32,
    height: u32,
    theme: Option<&crate::Theme>,
) -> Result<crate::Element<'static>, IconError> {
    let is_dark = is_dark_theme(theme);
    let element: crate::Element<'static> = svg_element(icon, width, height, is_dark).into();
    Ok(element)
}

#[allow(dead_code)]
fn invert_rgba(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks(4)
        .flat_map(|chunk| {
            if chunk[3] == 0 {
                chunk.to_vec()
            } else {
                vec![255 - chunk[0], 255 - chunk[1], 255 - chunk[2], chunk[3]]
            }
        })
        .collect()
}

// ─── 仅供 canvas svg_handle 使用的栅格化管线（保留以兼容非 widget 场景） ───

fn render_svg(
    svg_data: &[u8],
    target_width: u32,
    target_height: u32,
) -> Result<IconData, IconError> {
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg_data, &options)
        .map_err(|e| IconError::SvgParseError(e.to_string()))?;

    let svg_size = tree.size();
    let svg_width = svg_size.width();
    let svg_height = svg_size.height();

    let scale_x = target_width as f32 / svg_width;
    let scale_y = target_height as f32 / svg_height;
    // 等比缩放（contain），保持图标原始宽高比，避免拉伸变形
    let scale = scale_x.min(scale_y);

    // 缩放后的内容尺寸；不足目标画布时水平/垂直居中，避免贴左上角
    let scaled_w = svg_width * scale;
    let scaled_h = svg_height * scale;
    let offset_x = (target_width as f32 - scaled_w) / 2.0;
    let offset_y = (target_height as f32 - scaled_h) / 2.0;

    let transform = tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, offset_x, offset_y);

    let mut pixmap = tiny_skia::Pixmap::new(target_width, target_height)
        .ok_or(IconError::PixmapCreationError)?;
    pixmap.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 0));

    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let rgba = pixmap.data().to_vec();
    Ok(IconData {
        rgba,
        width: target_width,
        height: target_height,
    })
}

#[cfg(test)]
mod tests;
