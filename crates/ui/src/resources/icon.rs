use iced_widget::{Svg, svg};

use crate::Theme;

pub use Icon::*;

macro_rules! include_res {
    ($path:literal) => {
        include_bytes!(concat!("../../../../resources/icons/", $path))
    };
}

#[derive(Debug, Clone, Copy)]
pub enum Icon {
    /* FA Regular Icons start */
    AngleRight,
    FolderTree,
    Gear,
    WaveForm,
    /* FA Regular Icons end */
    /* FA Brands Icons start */
    GitHub,
    /* FA Brands Icons end */
    /* Window Traffic Icons start */
    WindowMin,
    WindowMax,
    WindowUnMax,
    WindowClose,
    /* Window Traffic Icons end */
}

pub fn view<'a>(icon: Icon) -> Svg<'a> {
    svg(svg::Handle::from_memory(bytes(icon))).style(|theme: &Theme, _| {
        let palette = theme.extended_palette();
        svg::Style {
            color: Some(palette.background.neutral.text),
        }
    })
}

fn bytes(icon: Icon) -> &'static [u8] {
    match icon {
        AngleRight => include_res!("regular/angle-right.svg"),
        FolderTree => include_res!("regular/folder-tree.svg"),
        Gear => include_res!("regular/gear.svg"),
        WaveForm => include_res!("regular/waveform.svg"),

        GitHub => include_res!("brands/github.svg"),

        WindowMin => include_res!("window/min.svg"),
        WindowMax => include_res!("window/max.svg"),
        WindowUnMax => include_res!("window/unmax.svg"),
        WindowClose => include_res!("window/close.svg"),
    }
}
