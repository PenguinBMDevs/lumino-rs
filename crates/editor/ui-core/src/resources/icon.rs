use iced_core::image::Handle;
use iced_widget::image::Image;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// 当前 HiDPI 图标渲染状态（true=2x，false=1x）
static HIDPI_ENABLED: AtomicBool = AtomicBool::new(true);

/// 缓存的 Handle 对象，key=(Icon, 是否暗色主题)。
/// 避免每帧创建新 Handle → iced_wgpu 缓存命中 → 零每帧纹理上传。
static HANDLE_CACHE: Lazy<Mutex<HashMap<(Icon, bool, u32), Handle>>> =
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

// ─── 图标定义宏：一处定义 → 三处生成（枚举 + 缓存构建 + bytes 匹配） ───
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

        fn build_icon_cache() -> HashMap<Icon, IconData> {
            let mut map = HashMap::new();
            $(
                match render_svg_to_data(Icon::$name) {
                    Ok(data) => { map.insert(Icon::$name, data); }
                    Err(e) => { tracing::error!("加载图标 {:?} 失败: {}", Icon::$name, e); }
                }
            )*
            map
        }

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
    // 颜料桶右侧的「绘制工具选择面板」触发小三角
    (ToolPanelCaret, "../../../../../resources/icons/toolbar/caret-down.svg"),
    // 绘制工具选择面板条目图标
    (StrokeSettings, "../../../../../resources/icons/toolbar/stroke-settings.svg"),
    (BrushTool, "../../../../../resources/icons/toolbar/brush-tool.svg"),
    (ShapeTool, "../../../../../resources/icons/toolbar/shape-tool.svg"),
    (TextInput, "../../../../../resources/icons/toolbar/text-input.svg"),
    // 画刷下拉/绘制行为对话框：圆形 +/- 按钮（SVG 绘制）
    (PlusCircle, "../../../../../resources/icons/toolbar/plus-circle.svg"),
    (MinusCircle, "../../../../../resources/icons/toolbar/minus-circle.svg"),
    (Quantize, "../../../../../resources/icons/toolbar/note-quantize.svg"),
    (Speed, "../../../../../resources/icons/toolbar/playback-speed.svg"),
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

static ICON_CACHE: Lazy<Mutex<HashMap<Icon, IconData>>> =
    Lazy::new(|| Mutex::new(build_icon_cache()));

/// 返回当前渲染倍率：HiDPI=2x，普通=1x
fn get_current_scale() -> u32 {
    if HIDPI_ENABLED.load(Ordering::Relaxed) {
        2
    } else {
        1
    }
}

/// 设置 HiDPI 状态并重建图标缓存
pub fn set_hidpi_enabled(enabled: bool) {
    HIDPI_ENABLED.store(enabled, Ordering::Relaxed);
    let new_cache = build_icon_cache();
    if let Ok(mut cache) = ICON_CACHE.lock() {
        *cache = new_cache;
    }
    // 清空 Handle 缓存，下次 view 调用时以新尺寸重建 Handle → iced_wgpu 重新上传纹理
    if let Ok(mut handle_cache) = HANDLE_CACHE.lock() {
        handle_cache.clear();
    }
}

/// 获取图标数据，如果不在缓存中则返回错误
#[allow(dead_code)]
fn get_icon_data(icon: Icon) -> Result<IconData, IconError> {
    let cache = ICON_CACHE.lock().map_err(|_| IconError::LockError)?;
    cache
        .get(&icon)
        .cloned()
        .ok_or(IconError::IconNotInCache(icon))
}

/// 获取或创建缓存的 Handle（默认不裁切，crop=1.0）。
/// 稳定 Handle::id() → iced_wgpu 的纹理缓存命中 → 零每帧图集上传。
fn get_or_create_handle(icon: Icon, is_dark: bool) -> Result<Handle, IconError> {
    get_or_create_handle_crop(icon, is_dark, 1.0)
}

/// 判断指定图标在当前主题下是否需要反色。
/// Logo 类图标（Lumino / LogoInApp）在暗色/亮色模式下均保持原色，不反色。
fn should_invert_icon(icon: Icon, is_dark: bool) -> bool {
    // MixerActive 为「亮灯」态固定琥珀色，不参与反色，确保打开面板时图标常亮。
    is_dark && !matches!(icon, Icon::Lumino | Icon::LogoInApp | Icon::MixerActive)
}

/// 渲染图标（可能 panic，仅用于向后兼容）
pub fn view(icon: Icon) -> crate::Element<'static> {
    match view_safe(icon) {
        Ok(element) => element,
        Err(e) => {
            tracing::error!("渲染图标失败: {}", e);
            // 返回一个空的占位符元素
            iced_widget::Space::new()
                .width(iced_core::Length::Fixed(24.0))
                .height(iced_core::Length::Fixed(24.0))
                .into()
        }
    }
}

/// 安全地渲染图标，返回 Result
pub fn view_safe(icon: Icon) -> Result<crate::Element<'static>, IconError> {
    let handle = get_or_create_handle(icon, false)?;
    Ok(Image::new(handle)
        .width(24)
        .height(24)
        .filter_method(iced_widget::image::FilterMethod::Nearest)
        .into())
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
            // 返回一个空的占位符元素
            iced_widget::Space::new()
                .width(iced_core::Length::Fixed(width as f32))
                .height(iced_core::Length::Fixed(height as f32))
                .into()
        }
    }
}

/// 渲染指定尺寸/主题且带居中裁切的图标（可能 panic，仅用于向后兼容）
pub fn view_with_size_and_theme_crop(
    icon: Icon,
    width: u32,
    height: u32,
    theme: Option<&crate::Theme>,
    crop: f32,
) -> crate::Element<'static> {
    match view_with_size_and_theme_crop_safe(icon, width, height, theme, crop) {
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
    let data = render_svg(svg_data, size, size, 1.0)?;
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
    view_with_size_and_theme_crop_safe(icon, width, height, theme, 1.0)
}

/// 安全地渲染指定尺寸和主题的图标，并可对 SVG 视口做居中裁切缩放，返回 Result
///
/// `crop` ∈ (0, 1]：仅取 SVG 视口中央 `crop` 比例的区域进行渲染，其余边缘留白被裁掉。
/// 许多 FontAwesome 图标的实际笔画只占视口的 ~80%（四周有固定留白），直接用
/// `contain` 渲染会导致「图标盒子很大、里面笔画却偏小」。传入 `crop≈0.8` 可让笔画
/// 填满图标盒子，视觉上更饱满、与工具栏标准图标风格一致。
pub fn view_with_size_and_theme_crop_safe(
    icon: Icon,
    width: u32,
    height: u32,
    theme: Option<&crate::Theme>,
    crop: f32,
) -> Result<crate::Element<'static>, IconError> {
    let is_dark = if crate::theme::is_high_contrast() {
        true
    } else {
        theme
            .map(|t| t.extended_palette().background.weakest.color.r < 0.5)
            .unwrap_or(true)
    };

    // 使用缓存 Handle（含主题反色），iced_wgpu 的纹理缓存命中后零每帧上传
    let handle = get_or_create_handle_crop(icon, is_dark, crop)?;

    Ok(Image::new(handle)
        .width(width)
        .height(height)
        .filter_method(iced_widget::image::FilterMethod::Nearest)
        .into())
}

/// 获取或创建带「居中裁切」的缓存 Handle。
///
/// `crop` 仅影响 SVG 有效笔画区域（裁掉四周留白），不改变主题反色结果；
/// 按 (icon, is_dark, 量化crop) 缓存，避免每帧重新光栅化。iced_wgpu 的纹理缓存
/// 命中后零每帧图集上传。
fn get_or_create_handle_crop(icon: Icon, is_dark: bool, crop: f32) -> Result<Handle, IconError> {
    let crop_q = (crop * 100.0).round().clamp(50.0, 100.0) as u32;
    let mut cache = HANDLE_CACHE.lock().map_err(|_| IconError::LockError)?;
    if let Some(handle) = cache.get(&(icon, is_dark, crop_q)) {
        return Ok(handle.clone());
    }

    // 裁切在光栅化阶段完成：直接按 crop 重新光栅化 SVG 字节
    let icon_data = render_svg_to_data_crop(icon, crop)?;
    let rgba = if should_invert_icon(icon, is_dark) {
        invert_rgba(&icon_data.rgba)
    } else {
        icon_data.rgba
    };
    let handle = Handle::from_rgba(icon_data.width, icon_data.height, rgba);
    cache.insert((icon, is_dark, crop_q), handle.clone());
    Ok(handle)
}

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

fn render_svg_to_data(icon: Icon) -> Result<IconData, IconError> {
    render_svg_to_data_crop(icon, 1.0)
}

/// 光栅化图标并支持居中裁切（`crop` 仅取 SVG 视口中央 `crop` 比例区域，裁掉四周留白）
fn render_svg_to_data_crop(icon: Icon, crop: f32) -> Result<IconData, IconError> {
    let svg_data = bytes(icon);
    let scale = get_current_scale();
    let size = match icon {
        Icon::WindowMin | Icon::WindowMax | Icon::WindowUnMax | Icon::WindowClose => 20 * scale,
        _ => 24 * scale,
    };
    render_svg(svg_data, size, size, crop)
}

fn render_svg(
    svg_data: &[u8],
    target_width: u32,
    target_height: u32,
    crop: f32,
) -> Result<IconData, IconError> {
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg_data, &options)
        .map_err(|e| IconError::SvgParseError(e.to_string()))?;

    let svg_size = tree.size();
    let svg_width = svg_size.width();
    let svg_height = svg_size.height();

    // 居中裁切：仅取视口中央 crop 比例区域作为有效内容，其余边缘留白被裁掉
    let crop = crop.clamp(0.5, 1.0);
    let view_w = svg_width * crop;
    let view_h = svg_height * crop;
    let view_off_x = (svg_width - view_w) / 2.0;
    let view_off_y = (svg_height - view_h) / 2.0;

    let scale_x = target_width as f32 / view_w;
    let scale_y = target_height as f32 / view_h;
    // 等比缩放（contain），保持图标原始宽高比，避免拉伸变形
    let scale = scale_x.min(scale_y);

    // 缩放后的有效内容尺寸；不足目标画布时水平/垂直居中，避免贴左上角
    let scaled_w = view_w * scale;
    let scaled_h = view_h * scale;
    let offset_x = (target_width as f32 - scaled_w) / 2.0;
    let offset_y = (target_height as f32 - scaled_h) / 2.0;

    // 将 view 坐标 (view_off_x, view_off_y) 映射到 pixmap (offset_x, offset_y)，
    // 使中央 crop 区域充满画布、四周留白被裁掉。
    let transform = tiny_skia::Transform::from_row(
        scale,
        0.0,
        0.0,
        scale,
        offset_x - view_off_x * scale,
        offset_y - view_off_y * scale,
    );

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
