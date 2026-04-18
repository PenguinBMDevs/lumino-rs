use once_cell::sync::Lazy;
use std::collections::HashMap;

use crate::resources::icon::{Icon, IconError, bytes::bytes};

pub struct IconData {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
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
        Icon::Play,
        Icon::Pause,
        Icon::SkipBackward,
        Icon::SkipForward,
        Icon::Undo,
        Icon::Redo,
        Icon::MousePointer,
        Icon::Pencil,
        Icon::Eraser,
        Icon::ArrowsLeftRight,
        Icon::Scroll,
        Icon::Ban,
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

pub fn get_icon_data(icon: Icon) -> Result<&'static IconData, IconError> {
    ICON_CACHE.get(&icon).ok_or(IconError::IconNotInCache(icon))
}

pub fn render_svg_to_data(icon: Icon) -> Result<IconData, IconError> {
    let svg_data = bytes(icon);
    let size = match icon {
        Icon::WindowMin | Icon::WindowMax | Icon::WindowUnMax | Icon::WindowClose => 20,
        _ => 24,
    };
    render_svg(svg_data, size, size)
}

pub fn render_svg(
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
