// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod logging;
mod pages;
mod resources;
mod ui;

use app::{
    App,
    window::{Settings, settings::PlatformSpecific},
};

fn main() -> iced::Result {
    logging::init();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Hello Lumino!");

    #[cfg(windows)]
    let platform_settings = Settings {
        // Disable native titlebar.
        decorations: false,
        platform_specific: PlatformSpecific {
            // Allows the OS to draw a shadow + frame on an undecorated window.
            // Improves UX when `decorations` is false.
            undecorated_shadow: true,
            ..Default::default()
        },
        ..Default::default()
    };

    #[cfg(target_os = "macos")]
    let platform_settings = Settings {
        platform_specific: PlatformSpecific {
            // Allows the content to be integrated with native titlebar.
            fullsize_content_view: true,
            // Make native titlebar transparent.
            titlebar_transparent: true,
            ..Default::default()
        },
        ..Default::default()
    };

    iced::application(App::new, App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .settings(iced::Settings {
            id: Some("com.buickmeow.lumino".into()),
            ..Default::default()
        })
        .window(Settings {
            min_size: Some(iced::Size {
                width: 800.0,
                height: 600.0,
            }),
            ..platform_settings
        })
        .run()
}
