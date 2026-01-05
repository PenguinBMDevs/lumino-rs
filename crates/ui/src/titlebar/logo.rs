use iced_core::Length;
use iced_widget::container;

use crate::{
    Element,
    resources::icon
};

pub fn view<'a>() -> Element<'a> {
    // simply just a placeholer
    let icon = icon(icon::GitHub)
        // 16+2=20-2
        .width(18)
        .height(18);

    container(icon)
        // 8+46+8
        .width(62)
        .height(Length::Fill)
        .center(62)
        .into()
}
