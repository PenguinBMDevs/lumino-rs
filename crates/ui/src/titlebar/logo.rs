use iced_core::{Alignment, Length};
use iced_widget::container;

use crate::{Element, resources::icon, window};

pub fn view(window: &window::Window) -> Element<'_> {
    let icon = icon::view_with_size_and_theme(icon::GitHub, 18, 18, Some(&window.theme));

    container(icon)
        .width(48)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .align_x(Alignment::Center)
        .into()
}
