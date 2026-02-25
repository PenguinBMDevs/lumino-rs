use iced_core::image::Handle;
use iced_widget::image::Image;
use image::GenericImageView;
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
        let data = icon.data();
        let img = image::load_from_memory(data).expect("Failed to load PNG");
        let (w, h) = img.dimensions();
        let rgba = img.to_rgba8().into_raw();
        map.insert(icon, Handle::from_rgba(w, h, rgba));
    }
    map
});

pub fn view(icon: Icon) -> crate::Element<'static> {
    let handle = ICON_CACHE.get(&icon).expect("Icon not in cache");
    Image::new(handle.clone())
        .filter_method(iced_widget::image::FilterMethod::Nearest)
        .into()
}

impl Icon {
    pub fn with_size(self, width: u32, height: u32) -> crate::Element<'static> {
        let handle = ICON_CACHE.get(&self).expect("Icon not in cache");
        Image::new(handle.clone())
            .width(width)
            .height(height)
            .filter_method(iced_widget::image::FilterMethod::Nearest)
            .into()
    }

    fn data(&self) -> &'static [u8] {
        match self {
            Icon::AngleRight => {
                include_bytes!("../../../../resources/icons/regular/angle-right.png")
            }
            Icon::FolderTree => {
                include_bytes!("../../../../resources/icons/regular/folder-tree.png")
            }
            Icon::Gear => include_bytes!("../../../../resources/icons/regular/gear.png"),
            Icon::WaveForm => include_bytes!("../../../../resources/icons/regular/waveform.png"),
            Icon::GitHub => include_bytes!("../../../../resources/icons/brands/github.png"),
            Icon::WindowMin => include_bytes!("../../../../resources/icons/window/min.png"),
            Icon::WindowMax => include_bytes!("../../../../resources/icons/window/max.png"),
            Icon::WindowUnMax => include_bytes!("../../../../resources/icons/window/unmax.png"),
            Icon::WindowClose => include_bytes!("../../../../resources/icons/window/close.png"),
        }
    }
}
