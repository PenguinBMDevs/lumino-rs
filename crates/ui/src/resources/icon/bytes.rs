use crate::resources::icon::Icon;

pub fn bytes(icon: Icon) -> &'static [u8] {
    match icon {
        Icon::AngleRight => {
            include_bytes!("../../../../../resources/icons/regular/angle-right.svg")
        }
        Icon::FolderTree => {
            include_bytes!("../../../../../resources/icons/regular/folder-tree.svg")
        }
        Icon::Gear => include_bytes!("../../../../../resources/icons/regular/gear.svg"),
        Icon::WaveForm => include_bytes!("../../../../../resources/icons/regular/waveform.svg"),
        Icon::GitHub => include_bytes!("../../../../../resources/icons/brands/github.svg"),
        Icon::WindowMin => include_bytes!("../../../../../resources/icons/window/min.svg"),
        Icon::WindowMax => include_bytes!("../../../../../resources/icons/window/max.svg"),
        Icon::WindowUnMax => include_bytes!("../../../../../resources/icons/window/unmax.svg"),
        Icon::WindowClose => include_bytes!("../../../../../resources/icons/window/close.svg"),
        Icon::Clock => include_bytes!("../../../../../resources/icons/sidebar/clock.svg"),
        Icon::Eye => include_bytes!("../../../../../resources/icons/sidebar/eye.svg"),
        Icon::EyeSlash => include_bytes!("../../../../../resources/icons/sidebar/eye-slash.svg"),
        Icon::Plus => include_bytes!("../../../../../resources/icons/sidebar/plus.svg"),
        Icon::EllipsisVertical => {
            include_bytes!("../../../../../resources/icons/sidebar/ellipsis-vertical.svg")
        }
        Icon::Users => include_bytes!("../../../../../resources/icons/toolbar/users.svg"),
        Icon::Play => include_bytes!("../../../../../resources/icons/toolbar/play.svg"),
        Icon::Pause => include_bytes!("../../../../../resources/icons/toolbar/pause.svg"),
        Icon::SkipBackward => {
            include_bytes!("../../../../../resources/icons/toolbar/skip-backward.svg")
        }
        Icon::SkipForward => {
            include_bytes!("../../../../../resources/icons/toolbar/skip-forward.svg")
        }
        Icon::Undo => include_bytes!("../../../../../resources/icons/toolbar/undo.svg"),
        Icon::Redo => include_bytes!("../../../../../resources/icons/toolbar/redo.svg"),
        Icon::MousePointer => {
            include_bytes!("../../../../../resources/icons/toolbar/mouse-pointer.svg")
        }
        Icon::Pencil => include_bytes!("../../../../../resources/icons/toolbar/pencil.svg"),
        Icon::Eraser => include_bytes!("../../../../../resources/icons/toolbar/eraser.svg"),
        Icon::ArrowsLeftRight => {
            include_bytes!("../../../../../resources/icons/toolbar/arrows-left-right.svg")
        }
        Icon::Scroll => include_bytes!("../../../../../resources/icons/toolbar/scroll.svg"),
        Icon::Ban => include_bytes!("../../../../../resources/icons/toolbar/ban.svg"),
    }
}
