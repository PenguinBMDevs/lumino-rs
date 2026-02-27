use iced_core::{Alignment, Length};
use iced_widget::{container, mouse_area};

use crate::{Element, resources::icon, window};

pub fn view(window: &window::Window) -> Element<'_> {
    let icon = icon::view_with_size_and_theme(icon::GitHub, 18, 18, Some(&window.theme));

    let logo_container = container(icon)
        .width(48)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .align_x(Alignment::Center);

    mouse_area(logo_container)
        .on_double_click(window::Event::close())
        .into()
}
