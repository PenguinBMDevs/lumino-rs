use iced_core::{Alignment, Length};
use iced_widget::container;

use crate::{Element, resources::icon};

pub fn view<'a>() -> Element<'a> {
    // simply just a placeholer
    let icon = icon(icon::GitHub)
        // 16+2=20-2
        .width(18);

    container(icon)
        .width(48)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .align_x(Alignment::Center)
        .into()
}
