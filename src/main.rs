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

    let api = lumino_midi::new_api(&lumino_midi::ApiKind::Kdmapi { path: "OmniMIDI.dll".into() }).unwrap();
    // let api = lumino_midi::new_api(&lumino_midi::ApiKind::System).unwrap();
    let outputs = api.outputs().unwrap();
    tracing::info!(?outputs, "Outputs");
    let inputs = api.inputs().unwrap();
    tracing::info!(?inputs, "Inputs");
    let mut conn = api.open_output(0).unwrap();
    for n in [60, 62, 64, 65, 67] {
        conn.note_on(1, n, 100).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        conn.note_on(1, n, 0).unwrap();
    }

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
