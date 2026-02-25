use iced_core::{Alignment, Length};
use iced_widget::container;

use crate::{Element, resources::icon};

pub fn view<'a>() -> Element<'a> {
    let icon = icon::GitHub.with_size(18, 18);

    container(icon)
        .width(48)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .align_x(Alignment::Center)
        .into()
}
