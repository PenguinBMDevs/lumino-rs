use iced_core::image::Handle;
use iced_widget::image::Image;
use once_cell::sync::Lazy;
use std::collections::HashMap;

pub use Icon::*;

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
    ] {
        let data = render_svg_to_data(icon);
        map.insert(icon, data);
    }
    map
});

pub fn view(icon: Icon) -> crate::Element<'static> {
    let data = ICON_CACHE.get(&icon).expect("Icon not in cache");
    let handle = Handle::from_rgba(data.width, data.height, data.rgba.clone());
    Image::new(handle)
        .filter_method(iced_widget::image::FilterMethod::Nearest)
        .into()
}

pub fn view_with_size(icon: Icon, width: u32, height: u32) -> crate::Element<'static> {
    view_with_size_and_theme(icon, width, height, None)
}

pub fn view_with_size_and_theme(
    icon: Icon,
    width: u32,
    height: u32,
    theme: Option<&crate::Theme>,
) -> crate::Element<'static> {
    let data = ICON_CACHE.get(&icon).expect("Icon not in cache");
    let is_dark = theme
        .map(|t| t.extended_palette().background.weakest.color.r < 0.5)
        .unwrap_or(true);

    // SVG图标使用currentColor（默认黑色），暗色主题需要反色为白色
    let rgba = if is_dark {
        invert_rgba(&data.rgba)
    } else {
        data.rgba.clone()
    };

    // 修复：使用缓存数据的原始尺寸创建 Handle，通过 Image widget 的 width/height 进行显示缩放
    let handle = Handle::from_rgba(data.width, data.height, rgba);

    Image::new(handle)
        .width(width)
        .height(height)
        .filter_method(iced_widget::image::FilterMethod::Nearest)
        .into()
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

fn render_svg_to_data(icon: Icon) -> IconData {
    let svg_data = bytes(icon);
    let size = match icon {
        Icon::WindowMin | Icon::WindowMax | Icon::WindowUnMax | Icon::WindowClose => 20,
        _ => 24,
    };
    render_svg(svg_data, size, size)
}

fn render_svg(svg_data: &[u8], target_width: u32, target_height: u32) -> IconData {
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg_data, &options).expect("Failed to parse SVG");

    let svg_size = tree.size();
    let svg_width = svg_size.width() as f32;
    let svg_height = svg_size.height() as f32;

    let scale_x = target_width as f32 / svg_width;
    let scale_y = target_height as f32 / svg_height;
    let scale = scale_x.min(scale_y);

    let transform = tiny_skia::Transform::from_scale(scale, scale);

    let mut pixmap =
        tiny_skia::Pixmap::new(target_width, target_height).expect("Failed to create pixmap");
    pixmap.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 0));

    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let rgba = pixmap.data().to_vec();
    IconData {
        rgba,
        width: target_width,
        height: target_height,
    }
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
    }
}
