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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    AngleRight,
    FolderTree,
    Arrangement,
    Gear,
    WaveForm,
    GitHub,
    WindowMin,
    WindowMax,
    WindowUnMax,
    WindowClose,
    Clock,
    Eye,
    EyeSlash,
    Plus,
    EllipsisVertical,
    Users,
    // 工具栏图标
    Play,
    Pause,
    SkipBackward,
    SkipForward,
    Undo,
    Redo,
    MousePointer,
    Pencil,
    Eraser,
    Quantize,
    // 自动滚动图标
    ArrowsLeftRight,
    Scroll,
    Ban,
    // 标题栏图标
    PencilOutline,
    Keys,
}

#[derive(Clone)]
struct IconData {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

static ICON_CACHE: Lazy<Mutex<HashMap<Icon, IconData>>> =
    Lazy::new(|| Mutex::new(build_icon_cache()));

/// 构建完整的图标缓存（使用当前 HIDPI_ENABLED 状态决定渲染倍率）
fn build_icon_cache() -> HashMap<Icon, IconData> {
    let mut map = HashMap::new();
    for &icon in &[
        Icon::AngleRight,
        Icon::FolderTree,
        Icon::Arrangement,
        Icon::Gear,
        Icon::WaveForm,
        Icon::GitHub,
        Icon::WindowMin,
        Icon::WindowMax,
        Icon::WindowUnMax,
        Icon::WindowClose,
        Icon::Clock,
        Icon::Eye,
        Icon::EyeSlash,
        Icon::Plus,
        Icon::EllipsisVertical,
        Icon::Users,
        // 工具栏图标
        Icon::Play,
        Icon::Pause,
        Icon::SkipBackward,
        Icon::SkipForward,
        Icon::Undo,
        Icon::Redo,
        Icon::MousePointer,
        Icon::Pencil,
        Icon::Eraser,
        Icon::Quantize,
        // 自动滚动图标
        Icon::ArrowsLeftRight,
        Icon::Scroll,
        Icon::Ban,
        // 标题栏图标
        Icon::PencilOutline,
        Icon::Keys,
    ] {
        match render_svg_to_data(icon) {
            Ok(data) => {
                map.insert(icon, data);
            }
            Err(e) => {
                tracing::error!("加载图标 {:?} 失败: {}", icon, e);
            }
        }
    }
    map
}

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
    let rgba = if is_dark {
        invert_rgba(&data.rgba)
    } else {
        data.rgba
    };
    let handle = Handle::from_rgba(data.width, data.height, rgba);
    cache.insert((icon, is_dark), handle.clone());
    Ok(handle)
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
    let is_dark = theme
        .map(|t| t.extended_palette().background.weakest.color.r < 0.5)
        .unwrap_or(true);

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

fn bytes(icon: Icon) -> &'static [u8] {
    match icon {
        Icon::AngleRight => include_bytes!("../../../../resources/icons/regular/angle-right.svg"),
        Icon::FolderTree => include_bytes!("../../../../resources/icons/regular/folder-tree.svg"),
        Icon::Arrangement => include_bytes!("../../../../resources/icons/regular/arrangement.svg"),
        Icon::Gear => include_bytes!("../../../../resources/icons/regular/gear.svg"),
        Icon::WaveForm => include_bytes!("../../../../resources/icons/regular/waveform.svg"),
        Icon::GitHub => include_bytes!("../../../../resources/icons/brands/github.svg"),
        Icon::WindowMin => include_bytes!("../../../../resources/icons/window/min.svg"),
        Icon::WindowMax => include_bytes!("../../../../resources/icons/window/max.svg"),
        Icon::WindowUnMax => include_bytes!("../../../../resources/icons/window/unmax.svg"),
        Icon::WindowClose => include_bytes!("../../../../resources/icons/window/close.svg"),
        Icon::Clock => include_bytes!("../../../../resources/icons/sidebar/clock.svg"),
        Icon::Eye => include_bytes!("../../../../resources/icons/sidebar/eye.svg"),
        Icon::EyeSlash => include_bytes!("../../../../resources/icons/sidebar/eye-slash.svg"),
        Icon::Plus => include_bytes!("../../../../resources/icons/sidebar/plus.svg"),
        Icon::EllipsisVertical => {
            include_bytes!("../../../../resources/icons/sidebar/ellipsis-vertical.svg")
        }
        Icon::Users => include_bytes!("../../../../resources/icons/toolbar/users.svg"),
        // 工具栏图标
        Icon::Play => include_bytes!("../../../../resources/icons/toolbar/play.svg"),
        Icon::Pause => include_bytes!("../../../../resources/icons/toolbar/pause.svg"),
        Icon::SkipBackward => {
            include_bytes!("../../../../resources/icons/toolbar/skip-backward.svg")
        }
        Icon::SkipForward => include_bytes!("../../../../resources/icons/toolbar/skip-forward.svg"),
        Icon::Undo => include_bytes!("../../../../resources/icons/toolbar/undo.svg"),
        Icon::Redo => include_bytes!("../../../../resources/icons/toolbar/redo.svg"),
        Icon::MousePointer => {
            include_bytes!("../../../../resources/icons/toolbar/mouse-pointer.svg")
        }
        Icon::Pencil => include_bytes!("../../../../resources/icons/toolbar/pencil.svg"),
        Icon::Eraser => include_bytes!("../../../../resources/icons/toolbar/eraser.svg"),
        Icon::Quantize => include_bytes!("../../../../resources/icons/toolbar/quantize.svg"),
        // 自动滚动图标
        Icon::ArrowsLeftRight => {
            include_bytes!("../../../../resources/icons/toolbar/arrows-left-right.svg")
        }
        Icon::Scroll => include_bytes!("../../../../resources/icons/toolbar/scroll.svg"),
        Icon::Ban => include_bytes!("../../../../resources/icons/toolbar/ban.svg"),
        // 标题栏图标
        Icon::PencilOutline => {
            include_bytes!("../../../../resources/icons/titlebar/pencil-outline.svg")
        }
        Icon::Keys => include_bytes!("../../../../resources/icons/titlebar/keys.svg"),
    }
}
