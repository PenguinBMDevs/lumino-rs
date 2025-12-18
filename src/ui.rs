pub mod window;
pub mod sidebar;

use crate::{
    app::{
        App,
        Message,
        router::Route
    },
    pages::{
        self,
        Page
    }
};

use iced::{
    Element, Length, widget::{
        column, container, row
    }
};

pub fn view<'a>(
    app: &App
) -> Element<'a, Message> {
    let content = match app.route {
        Route::Editor => app.pages.editor.view(),
        Route::Preview => app.pages.preview.view(),
        /* TODO */
        _ => app.pages.editor.view(),
    };

    let main = row![
        sidebar::view(&app.route),
        pages::view(content)
    ]
        .width(Length::Fill)
        .height(Length::Fill)
        .spacing(8)
        .padding(8);

    let inner = column![
        window::view(&app.window),
        main
    ];

    container(inner)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
