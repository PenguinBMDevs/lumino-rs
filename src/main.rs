// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod pages;
mod resources;
mod ui;
mod logging;

use app::{
    App,
    window::{Settings, settings::PlatformSpecific},
};

fn main() -> iced::Result {
    logging::init();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Hello Lumino!");

    lumino_midi::init(
        lumino_midi::MidiEngineType::Kdmapi,
        std::path::Path::new("OmniMidi.dll")
    ).expect("Failed to init kdmapi");

    tracing::info!(data = ?lumino_midi::version(), "KDMAPI Version");

    iced::application(App::new, App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .settings(iced::Settings {
            antialiasing: true,
            ..Default::default()
        })
        .window(Settings {
            size: iced::Size {
                width: 1024.0,
                height: 768.0,
            },
            min_size: None,
            max_size: None,
            visible: true,
            resizable: true,
            // Disable native titlebar.
            decorations: false,
            transparent: false,

            platform_specific: PlatformSpecific {
                // Allows the OS to draw a shadow + frame on an undecorated window.
                // Improves UX when `decorations` is false.
                undecorated_shadow: true,
                ..Default::default()
            },
            ..Default::default()
        })
        .run()
}
