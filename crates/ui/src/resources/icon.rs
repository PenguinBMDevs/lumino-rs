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
}

static ICON_CACHE: Lazy<HashMap<Icon, Handle>> = Lazy::new(|| {
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
    ] {
        let handle = render_svg_to_handle(icon);
        map.insert(icon, handle);
    }
    map
});

pub fn view(icon: Icon) -> crate::Element<'static> {
    let handle = ICON_CACHE.get(&icon).expect("Icon not in cache");
    Image::new(handle.clone())
        .filter_method(iced_widget::image::FilterMethod::Nearest)
        .into()
}

pub fn view_with_size(icon: Icon, width: u32, height: u32) -> crate::Element<'static> {
    let handle = ICON_CACHE.get(&icon).expect("Icon not in cache");
    Image::new(handle.clone())
        .width(width)
        .height(height)
        .filter_method(iced_widget::image::FilterMethod::Nearest)
        .into()
}

fn render_svg_to_handle(icon: Icon) -> Handle {
    let svg_data = bytes(icon);
    let size = match icon {
        Icon::WindowMin | Icon::WindowMax | Icon::WindowUnMax | Icon::WindowClose => 20,
        _ => 24,
    };
    render_svg(svg_data, size, size)
}

fn render_svg(svg_data: &[u8], target_width: u32, target_height: u32) -> Handle {
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg_data, &options).expect("Failed to parse SVG");
    
    // 获取 SVG 原始尺寸
    let svg_size = tree.size();
    let svg_width = svg_size.width() as f32;
    let svg_height = svg_size.height() as f32;
    
    // 计算缩放比例，保持宽高比
    let scale_x = target_width as f32 / svg_width;
    let scale_y = target_height as f32 / svg_height;
    let scale = scale_x.min(scale_y);
    
    // 创建变换矩阵进行缩放
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    
    let mut pixmap = tiny_skia::Pixmap::new(target_width, target_height).expect("Failed to create pixmap");
    // 清空为透明背景
    pixmap.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 0));
    
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    
    let rgba = pixmap.data().to_vec();
    Handle::from_rgba(target_width, target_height, rgba)
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
    }
}
