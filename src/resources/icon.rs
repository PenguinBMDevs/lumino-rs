use iced::{
    Theme,
    widget::{Svg, svg},
};

pub use Icon::*;

macro_rules! include_res {
    ($path:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/resources/icons/",
            $path
        ))
    };
}

#[derive(Debug, Clone, Copy)]
pub enum Icon {
    /* FA Regular Icons start */
    ChartBar,
    FileLines,
    MusicNote,
    PenToSquare,
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
    svg(svg::Handle::from_memory(bytes(icon)))
        .width(16)
        .height(16)
        .style(|theme: &Theme, _| {
            let palette = theme.extended_palette();
            svg::Style {
                color: Some(palette.background.neutral.text),
            }
        })
}

fn bytes(icon: Icon) -> &'static [u8] {
    match icon {
        ChartBar => include_res!("regular/chart-bar.svg"),
        FileLines => include_res!("regular/file-lines.svg"),
        MusicNote => include_res!("regular/music-note.svg"),
        PenToSquare => include_res!("regular/pen-to-square.svg"),

        GitHub => include_res!("brands/github.svg"),

        WindowMin => include_res!("window/min.svg"),
        WindowMax => include_res!("window/max.svg"),
        WindowUnMax => include_res!("window/unmax.svg"),
        WindowClose => include_res!("window/close.svg"),
    }
}
