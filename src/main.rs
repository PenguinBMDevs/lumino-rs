// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod ui;

use app::{
    App,
    window::{
        settings::PlatformSpecific,
        Settings,
    }
};

use iced::{
    Size, Theme
};

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .settings(iced::Settings {
            antialiasing: true,
            ..Default::default()
        })
        .window(Settings {
            size: Size {
                width: 1024.0,
                height: 768.0,
            },
            min_size: None,
            max_size: None,
            visible: true,
            resizable: true,
            decorations: false,
            transparent: false,

            platform_specific: PlatformSpecific {
                undecorated_shadow: true,
                ..Default::default()
            },
            ..Default::default()
        })
        .theme(Theme::TokyoNight)
        .centered()
        .title("Lumino")
        .run()
}
