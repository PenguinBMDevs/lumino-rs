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
static HANDLE_CACHE: Lazy<Mutex<HashMap<(Icon, bool), Handle>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub use Icon::*;

/// 图标加载错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum IconError {
    #[error("图标 {0:?} 不在缓存中")]
    IconNotInCache(Icon),
    #[error("SVG 解析错误: {0}")]
    SvgParseError(String),
    #[error("无法创建 pixmap")]
    PixmapCreationError,
    #[error("获取缓存锁失败")]
    LockError,
}

// ─── 图标定义宏：一处定义 → 三处生成（枚举 + 缓存构建 + bytes 匹配） ───
macro_rules! define_icons {
    ($(($name:ident, $path:expr)),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Icon {
            $($name,)*
        }

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
    (AngleRight, "../../../../resources/icons/regular/submenu-expand-indicator.svg"),
    (FolderTree, "../../../../resources/icons/regular/ui-layout.svg"),
    (Arrangement, "../../../../resources/icons/regular/arrangement-mode.svg"),
    (Gear, "../../../../resources/icons/regular/settings-general.svg"),
    (WaveForm, "../../../../resources/icons/regular/audio-automation.svg"),
    (Lumino, "../../../../resources/icons/brands/lumino-brand.svg"),
    (LogoInApp, "../../../../resources/icons/brands/app-logo.svg"),
    (WindowMin, "../../../../resources/icons/window/min.svg"),
    (WindowMax, "../../../../resources/icons/window/max.svg"),
    (WindowUnMax, "../../../../resources/icons/window/unmax.svg"),
    (WindowClose, "../../../../resources/icons/window/close.svg"),
    (Clock, "../../../../resources/icons/sidebar/conductor-track.svg"),
    (Eye, "../../../../resources/icons/sidebar/onion-skin.svg"),
    (EyeSlash, "../../../../resources/icons/sidebar/onion-skin-disabled.svg"),
    (Plus, "../../../../resources/icons/sidebar/add-track.svg"),
    (Download, "../../../../resources/icons/sidebar/export-group.svg"),
    (PlayCircle, "../../../../resources/icons/sidebar/waterfall-record.svg"),
    (EllipsisVertical, "../../../../resources/icons/sidebar/toolbar-overflow-trigger.svg"),
    (Users, "../../../../resources/icons/toolbar/collaboration.svg"),
    // 工具栏图标
    (Play, "../../../../resources/icons/toolbar/playback-start.svg"),
    (Pause, "../../../../resources/icons/toolbar/playback-pause.svg"),
    (SkipBackward, "../../../../resources/icons/toolbar/playback-jump-start.svg"),
    (SkipForward, "../../../../resources/icons/toolbar/playback-jump-end.svg"),
    (Undo, "../../../../resources/icons/toolbar/history-undo.svg"),
    (Redo, "../../../../resources/icons/toolbar/history-redo.svg"),
    (MousePointer, "../../../../resources/icons/toolbar/select-tool.svg"),
    (MousePointerYSelect, "../../../../resources/icons/toolbar/y-axis-select-tool.svg"),
    (Pencil, "../../../../resources/icons/toolbar/note-draw-tool.svg"),
    (Eraser, "../../../../resources/icons/toolbar/eraser-tool.svg"),
    (Curve, "../../../../resources/icons/toolbar/curve-tool.svg"),
    (Quantize, "../../../../resources/icons/toolbar/note-quantize.svg"),
    (Speed, "../../../../resources/icons/toolbar/playback-speed.svg"),
    // 音符翻转图标
    (FlipVertical, "../../../../resources/icons/toolbar/note-flip-vertical.svg"),
    (FlipHorizontal, "../../../../resources/icons/toolbar/note-flip-horizontal.svg"),
    // 自动滚动图标
    (ArrowsLeftRight, "../../../../resources/icons/toolbar/loop-range-active.svg"),
    (Scroll, "../../../../resources/icons/toolbar/autoscroll-scrolling.svg"),
    (Ban, "../../../../resources/icons/toolbar/loop-range-disabled.svg"),
    // 移调/分割/合并 图标
    (TransposeUp, "../../../../resources/icons/toolbar/note-transpose-up.svg"),
    (TransposeDown, "../../../../resources/icons/toolbar/note-transpose-down.svg"),
    (Split, "../../../../resources/icons/toolbar/note-split.svg"),
    (Glue, "../../../../resources/icons/toolbar/note-glue.svg"),
    // 连奏/同音连接
    (Tie, "../../../../resources/icons/toolbar/note-tie.svg"),
    // 图片转 MIDI 占位图标
    (ImageToMidi, "../../../../resources/icons/toolbar/image-to-midi-converter.svg"),
    // 素材库图标（右侧栏）
    (MaterialLibrary, "../../../../resources/icons/toolbar/material-library.svg"),
    // 标题栏图标
    (PencilOutline, "../../../../resources/icons/titlebar/editor-mode.svg"),
    (Keys, "../../../../resources/icons/titlebar/piano-roll.svg"),
    (VideoCamera, "../../../../resources/icons/sidebar/video-export.svg"),
    (MusicNote, "../../../../resources/icons/sidebar/audio-export.svg"),
    // 钢琴卷帘右键上下文菜单图标
    (ContextMenuCut, "../../../../resources/icons/context-menu/cut-notes.svg"),
    (ContextMenuCopy, "../../../../resources/icons/context-menu/copy-notes.svg"),
    (ContextMenuPaste, "../../../../resources/icons/context-menu/paste-notes.svg"),
    (ContextMenuDelete, "../../../../resources/icons/context-menu/delete-item.svg"),
    (ContextMenuSelectAll, "../../../../resources/icons/context-menu/select-all-notes.svg"),
    (ContextMenuColorPalette, "../../../../resources/icons/context-menu/set-track-color.svg"),
    (ContextMenuChannel, "../../../../resources/icons/context-menu/set-midi-channel.svg"),
    (ContextMenuRecoverTrack, "../../../../resources/icons/context-menu/recover-deleted-track.svg"),
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
fn get_icon_data(icon: Icon) -> Result<IconData, IconError> {
    let cache = ICON_CACHE.lock().map_err(|_| IconError::LockError)?;
    cache
        .get(&icon)
        .cloned()
        .ok_or(IconError::IconNotInCache(icon))
}

/// 获取或创建缓存的 Handle。
/// 稳定 Handle::id() → iced_wgpu 的纹理缓存命中 → 零每帧图集上传。
fn get_or_create_handle(icon: Icon, is_dark: bool) -> Result<Handle, IconError> {
    let mut cache = HANDLE_CACHE.lock().map_err(|_| IconError::LockError)?;
    if let Some(handle) = cache.get(&(icon, is_dark)) {
        return Ok(handle.clone());
    }

    let icon_data = get_icon_data(icon)?;
    let rgba = if should_invert_icon(icon, is_dark) {
        invert_rgba(&icon_data.rgba)
    } else {
        icon_data.rgba
    };
    let handle = Handle::from_rgba(icon_data.width, icon_data.height, rgba);
    cache.insert((icon, is_dark), handle.clone());
    Ok(handle)
}

/// 判断指定图标在当前主题下是否需要反色。
/// Logo 类图标（Lumino / LogoInApp）在暗色/亮色模式下均保持原色，不反色。
fn should_invert_icon(icon: Icon, is_dark: bool) -> bool {
    is_dark && !matches!(icon, Icon::Lumino | Icon::LogoInApp)
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
    let is_dark = if crate::theme::is_high_contrast() {
        true
    } else {
        theme
            .map(|t| t.extended_palette().background.weakest.color.r < 0.5)
            .unwrap_or(true)
    };

    // 使用缓存 Handle（含主题反色），iced_wgpu 的纹理缓存命中后零每帧上传
    let handle = get_or_create_handle(icon, is_dark)?;

    Ok(Image::new(handle)
        .width(width)
        .height(height)
        .filter_method(iced_widget::image::FilterMethod::Nearest)
        .into())
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
    let svg_data = bytes(icon);
    let scale = get_current_scale();
    let size = match icon {
        Icon::WindowMin | Icon::WindowMax | Icon::WindowUnMax | Icon::WindowClose => 20 * scale,
        _ => 24 * scale,
    };
    render_svg(svg_data, size, size)
}

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

// bytes() 函数由 define_icons! 宏生成

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invert_rgba_inverts_opaque_pixels() {
        let rgba = vec![10, 20, 30, 255, 40, 50, 60, 255];
        let inverted = invert_rgba(&rgba);
        assert_eq!(inverted, vec![245, 235, 225, 255, 215, 205, 195, 255]);
    }

    #[test]
    fn test_invert_rgba_preserves_transparent_pixels() {
        let rgba = vec![10, 20, 30, 0, 40, 50, 60, 128];
        let inverted = invert_rgba(&rgba);
        // 完全透明像素保持原样，半透明像素仍做反色
        assert_eq!(inverted, vec![10, 20, 30, 0, 215, 205, 195, 128]);
    }

    #[test]
    fn test_logo_icons_never_invert() {
        assert!(!should_invert_icon(Icon::Lumino, true));
        assert!(!should_invert_icon(Icon::LogoInApp, true));
        assert!(!should_invert_icon(Icon::Lumino, false));
        assert!(!should_invert_icon(Icon::LogoInApp, false));
    }

    #[test]
    fn test_regular_icons_invert_only_in_dark_mode() {
        assert!(should_invert_icon(Icon::Gear, true));
        assert!(!should_invert_icon(Icon::Gear, false));
        assert!(should_invert_icon(Icon::Play, true));
        assert!(!should_invert_icon(Icon::Play, false));
    }

    /// 计算渲染结果中非透明像素的包围盒 (min_x, min_y, max_x, max_y)
    fn content_bbox(data: &IconData) -> (u32, u32, u32, u32) {
        let mut min_x = data.width;
        let mut min_y = data.height;
        let mut max_x = 0;
        let mut max_y = 0;
        for y in 0..data.height {
            for x in 0..data.width {
                let idx = ((y * data.width + x) * 4 + 3) as usize;
                if data.rgba[idx] > 0 {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x + 1);
                    max_y = max_y.max(y + 1);
                }
            }
        }
        (min_x, min_y, max_x, max_y)
    }

    /// 非正方形图标（20x10）渲染到 24x24 画布：等比缩放后水平铺满、垂直居中
    #[test]
    fn test_render_svg_centers_non_square_icon() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10" viewBox="0 0 20 10"><rect x="0" y="0" width="20" height="10" fill="#ff0000"/></svg>"##;
        let data = render_svg(svg, 24, 24).expect("渲染失败");
        // scale = min(24/20, 24/10) = 1.2 → 内容 24x12，y 方向居中：offset = (24-12)/2 = 6
        assert_eq!(content_bbox(&data), (0, 6, 24, 18));
    }

    /// 高瘦图标（10x20）渲染到 24x24 画布：垂直铺满、水平居中
    #[test]
    fn test_render_svg_centers_tall_icon() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20" viewBox="0 0 10 20"><rect x="0" y="0" width="10" height="20" fill="#00ff00"/></svg>"##;
        let data = render_svg(svg, 24, 24).expect("渲染失败");
        // scale = min(24/10, 24/20) = 1.2 → 内容 12x24，x 方向居中：offset = (24-12)/2 = 6
        assert_eq!(content_bbox(&data), (6, 0, 18, 24));
    }

    /// 正方形图标（24x24）渲染到 24x24 画布：铺满、无留白
    #[test]
    fn test_render_svg_square_icon_fills_canvas() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><rect x="0" y="0" width="24" height="24" fill="#0000ff"/></svg>"##;
        let data = render_svg(svg, 24, 24).expect("渲染失败");
        assert_eq!(content_bbox(&data), (0, 0, 24, 24));
    }

    /// 真实图标文件：note-tie（20x11.75 非正方形）渲染后内容在 24x24 中垂直居中
    #[test]
    fn test_render_real_note_tie_icon_is_centered() {
        let svg_data = super::bytes(Icon::Tie);
        let data = render_svg(svg_data, 24, 24).expect("渲染失败");
        let (min_x, min_y, max_x, max_y) = content_bbox(&data);
        // 内容必须完全落在 24x24 画布内（不裁剪、不越界）
        assert!(max_x <= 24 && max_y <= 24);
        // 等比缩放后 content 高度 < 24，应垂直居中：上下留白相等
        let top = min_y;
        let bottom = 24 - max_y;
        assert_eq!(top, bottom, "内容应垂直居中: top={top}, bottom={bottom}");
        // 水平方向应铺满或居中（note-tie 宽高比接近 20:11.75，按 24x24 缩放后可能贴边）
        let left = min_x;
        let right = 24 - max_x;
        assert_eq!(left, right, "内容应水平居中: left={left}, right={right}");
    }

    /// 全量回归：所有内置 SVG 都能解析并渲染，防止批量修改后出现非法文件
    #[test]
    fn test_all_icon_svgs_parse_and_render() {
        // 直接触发宏生成的缓存构建：任一 SVG 解析失败会缺失条目（构建函数内部只打日志不 panic）
        let cache = build_icon_cache();
        // 当前 define_icons! 宏内共 56 个图标；若宏新增条目此处需同步更新
        assert_eq!(
            cache.len(),
            56,
            "存在无法解析/渲染的 SVG 图标，请检查 resources/icons"
        );
    }

    /// 新增的 i2m 悬浮按钮图标（不注册进 Icon 枚举，直接验证 SVG 可解析且输出 32x32 RGBA 纹理）
    #[test]
    fn test_i2m_button_icons_parse() {
        let check_svg = include_bytes!("../../../../resources/icons/toolbar/confirm-check.svg");
        let cross_svg = include_bytes!("../../../../resources/icons/toolbar/cancel-cross.svg");
        for svg in [check_svg.as_slice(), cross_svg.as_slice()] {
            let handle = super::svg_handle(svg, 32).expect("i2m 按钮图标应能光栅化");
            match handle {
                iced_core::image::Handle::Rgba {
                    width,
                    height,
                    pixels,
                    ..
                } => {
                    assert_eq!(width, 32);
                    assert_eq!(height, 32);
                    assert_eq!(pixels.len(), 32 * 32 * 4);
                }
                other => panic!("应为 RGBA 纹理，实际为 {other:?}"),
            }
        }
    }

    /// viewBox 扩大 1.5 倍后，内容线性缩小 1/3 且在画布中居中
    #[test]
    fn test_render_svg_shrunken_viewbox_centers_content() {
        // 20x20 内容放进 30x30 viewBox：渲染到 24x24 应为 16x16 且居中
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="-5 -5 30 30"><rect x="0" y="0" width="20" height="20" fill="#ff0000"/></svg>"##;
        let data = render_svg(svg, 24, 24).expect("渲染失败");
        // scale = 24/30 = 0.8 → 内容 16x16，offset = (24-16)/2 = 4
        assert_eq!(content_bbox(&data), (4, 4, 20, 20));
    }

    /// 真实播放图标（playback-start）：缩小 1/3 后内容约 12.8x16 且居中
    #[test]
    fn test_play_icon_shrunk_to_two_thirds_and_centered() {
        let data = render_svg(super::bytes(Icon::Play), 24, 24).expect("渲染失败");
        let (min_x, min_y, max_x, max_y) = content_bbox(&data);
        let w = max_x - min_x;
        let h = max_y - min_y;
        // viewBox 10.39x13.00 → 15.58x19.50：scale = 24/19.50 ≈ 1.23 → 内容约 12.8x16.0
        assert!((w as f32 - 12.78).abs() <= 1.5, "宽度 {w} 应约为 12.78");
        assert!((h as f32 - 16.0).abs() <= 1.5, "高度 {h} 应约为 16.0");
        // 水平/垂直居中
        assert_eq!(
            min_x,
            24 - max_x,
            "内容应水平居中: left={min_x}, right={}",
            24 - max_x
        );
        assert_eq!(
            min_y,
            24 - max_y,
            "内容应垂直居中: top={min_y}, bottom={}",
            24 - max_y
        );
    }
}
