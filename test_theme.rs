fn main() {
    let t = iced_core::Theme::custom("Black", iced_core::palette::Palette {
        background: iced_core::Color::BLACK,
        text: iced_core::Color::WHITE,
        primary: iced_core::Color::from_rgb(1.0, 0.8, 0.0),
        success: iced_core::Color::from_rgb(0.0, 0.8, 0.2),
        danger: iced_core::Color::from_rgb(0.9, 0.1, 0.1),
    });
    println!("{}", t.to_string());
}
