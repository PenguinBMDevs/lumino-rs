use iced_core::image::Handle;
use iced_widget::image::Image;
use once_cell::sync::Lazy;
use std::collections::HashMap;

pub use Icon::*;

/// 图标加载错误类型
#[derive(Debug, Clone)]
pub enum IconError {
    IconNotInCache(Icon),
    SvgParseError(String),
    PixmapCreationError,
}

impl std::fmt::Display for IconError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IconError::IconNotInCache(icon) => write!(f, "图标 {:?} 不在缓存中", icon),
            IconError::SvgParseError(msg) => write!(f, "SVG 解析错误: {}", msg),
            IconError::PixmapCreationError => write!(f, "无法创建 pixmap"),
        }
    }
}

impl std::error::Error for IconError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    AngleRight,
    FolderTree,
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
    MousePointer,
    Pencil,
    Eraser,
}

struct IconData {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

static ICON_CACHE: Lazy<HashMap<Icon, IconData>> = Lazy::new(|| {
    let mut map = HashMap::new();
    for &icon in &[
        Icon::AngleRight,
        Icon::FolderTree,
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
        Icon::MousePointer,
        Icon::Pencil,
        Icon::Eraser,
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
});

/// 获取图标数据，如果不在缓存中则返回错误
fn get_icon_data(icon: Icon) -> Result<&'static IconData, IconError> {
    ICON_CACHE.get(&icon).ok_or(IconError::IconNotInCache(icon))
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
    let data = get_icon_data(icon)?;
    let handle = Handle::from_rgba(data.width, data.height, data.rgba.clone());
    Ok(Image::new(handle)
        .filter_method(iced_widget::image::FilterMethod::Nearest)
        .into())
}

/// 渲染指定尺寸的图标（可能 panic，仅用于向后兼容）
pub fn view_with_size(icon: Icon, width: u32, height: u32) -> crate::Element<'static> {
    view_with_size_and_theme(icon, width, height, None)
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
    let data = get_icon_data(icon)?;
    let is_dark = theme
        .map(|t| t.extended_palette().background.weakest.color.r < 0.5)
        .unwrap_or(true);

    // SVG图标使用currentColor（默认黑色），暗色主题需要反色为白色
    let rgba = if is_dark {
        invert_rgba(&data.rgba)
    } else {
        data.rgba.clone()
    };

    // 使用缓存数据的原始尺寸创建 Handle，通过 Image widget 的 width/height 进行显示缩放
    let handle = Handle::from_rgba(data.width, data.height, rgba);

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
    let size = match icon {
        Icon::WindowMin | Icon::WindowMax | Icon::WindowUnMax | Icon::WindowClose => 20,
        _ => 24,
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
        Icon::MousePointer => {
            include_bytes!("../../../../resources/icons/toolbar/mouse-pointer.svg")
        }
        Icon::Pencil => include_bytes!("../../../../resources/icons/toolbar/pencil.svg"),
        Icon::Eraser => include_bytes!("../../../../resources/icons/toolbar/eraser.svg"),
    }
}
