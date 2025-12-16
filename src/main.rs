// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod ui;

use ui::app::App;

use iced::{
    Size, Theme, window
};

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .settings(iced::Settings {
            /* TODO! */
            ..Default::default()
        })
        .window(window::Settings {
            size: Size {
                width: 1024.0,
                height: 768.0,
            },
            min_size: None,
            max_size: None,
            visible: true,
            resizable: true,
            decorations: true,
            transparent: false,

            /* TODO! */
            icon: None,
            ..Default::default()
        })
        .theme(Theme::TokyoNight)
        .centered()
        .title("Lumino")
        .run()
}
