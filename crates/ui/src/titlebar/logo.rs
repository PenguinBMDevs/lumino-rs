use iced_core::{Alignment, Length};
use iced_widget::container;

use crate::{Element, resources::icon};

pub fn view<'a>() -> Element<'a> {
    let icon = icon::view_with_size(icon::GitHub, 18, 18);

    container(icon)
        .width(48)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .align_x(Alignment::Center)
        .into()
}
