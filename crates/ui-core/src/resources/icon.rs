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
    (AngleRight, "../../../../resources/icons/regular/angle-right.svg"),
    (FolderTree, "../../../../resources/icons/regular/folder-tree.svg"),
    (Arrangement, "../../../../resources/icons/regular/arrangement.svg"),
    (Gear, "../../../../resources/icons/regular/gear.svg"),
    (WaveForm, "../../../../resources/icons/regular/waveform.svg"),
    (Lumino, "../../../../resources/icons/brands/Lumino.svg"),
    (LogoInApp, "../../../../resources/icons/brands/LogoInApp.svg"),
    (WindowMin, "../../../../resources/icons/window/min.svg"),
    (WindowMax, "../../../../resources/icons/window/max.svg"),
    (WindowUnMax, "../../../../resources/icons/window/unmax.svg"),
    (WindowClose, "../../../../resources/icons/window/close.svg"),
    (Clock, "../../../../resources/icons/sidebar/clock.svg"),
    (Eye, "../../../../resources/icons/sidebar/eye.svg"),
    (EyeSlash, "../../../../resources/icons/sidebar/eye-slash.svg"),
    (Plus, "../../../../resources/icons/sidebar/plus.svg"),
    (Download, "../../../../resources/icons/sidebar/download.svg"),
    (PlayCircle, "../../../../resources/icons/sidebar/play-circle.svg"),
    (EllipsisVertical, "../../../../resources/icons/sidebar/ellipsis-vertical.svg"),
    (EventList, "../../../../resources/icons/sidebar/event-list.svg"),
    (Pushpin, "../../../../resources/icons/sidebar/pushpin.svg"),
    (Users, "../../../../resources/icons/toolbar/users.svg"),
    // 工具栏图标
    (Play, "../../../../resources/icons/toolbar/play.svg"),
    (Pause, "../../../../resources/icons/toolbar/pause.svg"),
    (SkipBackward, "../../../../resources/icons/toolbar/skip-backward.svg"),
    (SkipForward, "../../../../resources/icons/toolbar/skip-forward.svg"),
    (Undo, "../../../../resources/icons/toolbar/undo.svg"),
    (Redo, "../../../../resources/icons/toolbar/redo.svg"),
    (MousePointer, "../../../../resources/icons/toolbar/mouse-pointer.svg"),
    (Pencil, "../../../../resources/icons/toolbar/pencil.svg"),
    (Eraser, "../../../../resources/icons/toolbar/eraser.svg"),
    (Curve, "../../../../resources/icons/toolbar/curve.svg"),
    (Quantize, "../../../../resources/icons/toolbar/quantize.svg"),
    (Speed, "../../../../resources/icons/toolbar/speed.svg"),
    // 音符翻转图标
    (FlipVertical, "../../../../resources/icons/toolbar/flip-vertical.svg"),
    (FlipHorizontal, "../../../../resources/icons/toolbar/flip-horizontal.svg"),
    // 自动滚动图标
    (ArrowsLeftRight, "../../../../resources/icons/toolbar/arrows-left-right.svg"),
    (Scroll, "../../../../resources/icons/toolbar/scroll.svg"),
    (Ban, "../../../../resources/icons/toolbar/ban.svg"),
    // 移调/分割/合并 图标
    (TransposeUp, "../../../../resources/icons/toolbar/transpose-up.svg"),
    (TransposeDown, "../../../../resources/icons/toolbar/transpose-down.svg"),
    (Split, "../../../../resources/icons/toolbar/split.svg"),
    (Glue, "../../../../resources/icons/toolbar/glue.svg"),
    // 连奏/同音连接
    (Tie, "../../../../resources/icons/toolbar/tie.svg"),
    // 标题栏图标
    (PencilOutline, "../../../../resources/icons/titlebar/pencil-outline.svg"),
    (Keys, "../../../../resources/icons/titlebar/keys.svg"),
    (VideoCamera, "../../../../resources/icons/sidebar/video-camera.svg"),
    (MusicNote, "../../../../resources/icons/sidebar/music-note.svg"),
    // 钢琴卷帘右键上下文菜单图标
    (ContextMenuCut, "../../../../resources/icons/context-menu/cut.svg"),
    (ContextMenuCopy, "../../../../resources/icons/context-menu/copy.svg"),
    (ContextMenuPaste, "../../../../resources/icons/context-menu/paste.svg"),
    (ContextMenuDelete, "../../../../resources/icons/context-menu/delete.svg"),
    (ContextMenuSelectAll, "../../../../resources/icons/context-menu/select-all.svg"),
    (ContextMenuColorPalette, "../../../../resources/icons/context-menu/color-palette.svg"),
    (ContextMenuChannel, "../../../../resources/icons/context-menu/channel.svg"),
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

    let data = get_icon_data(icon)?;
    let rgba = if should_invert_icon(icon, is_dark) {
        invert_rgba(&data.rgba)
    } else {
        data.rgba
    };
    let handle = Handle::from_rgba(data.width, data.height, rgba);
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
    let scale = scale_x.min(scale_y);

    let transform = tiny_skia::Transform::from_scale(scale, scale);

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
}
