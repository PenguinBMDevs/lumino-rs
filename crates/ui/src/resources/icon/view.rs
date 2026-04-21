use crate::resources::icon::Icon;
use crate::resources::icon::IconError;
use crate::resources::icon::cache::get_icon_data;
use crate::resources::icon::render::IconData;
use iced_core::image::Handle;
use iced_widget::image::Image;

fn invert_rgba(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks(4)
        .flat_map(|chunk| {
            if chunk[3] == 0 {
                chunk.to_vec()
            } else {
                vec![255 - chunk[0], 255 - chunk[1], 255 - chunk[2], chunk[3]]
            }
        })
        .collect()
}

pub fn view(icon: Icon) -> crate::Element<'static> {
    match view_safe(icon) {
        Ok(element) => element,
        Err(e) => {
            tracing::error!("渲染图标失败: {}", e);
            iced_widget::Space::new()
                .width(iced_core::Length::Fixed(24.0))
                .height(iced_core::Length::Fixed(24.0))
                .into()
        }
    }
}

pub fn view_safe(icon: Icon) -> Result<crate::Element<'static>, IconError> {
    let data = get_icon_data(icon)?;
    let handle = Handle::from_rgba(data.width, data.height, data.rgba.clone());
    Ok(Image::new(handle)
        .filter_method(iced_widget::image::FilterMethod::Linear)
        .into())
}

pub fn view_with_size_and_theme(
    icon: Icon,
    width: u32,
    height: u32,
    theme: Option<&crate::Theme>,
) -> crate::Element<'static> {
    match view_with_size_and_theme_safe(icon, width, height, theme) {
        Ok(element) => element,
        Err(e) => {
            tracing::error!("渲染图标失败: {}", e);
            iced_widget::Space::new()
                .width(iced_core::Length::Fixed(width as f32))
                .height(iced_core::Length::Fixed(height as f32))
                .into()
        }
    }
}

pub fn view_with_size_and_theme_safe(
    icon: Icon,
    width: u32,
    height: u32,
    theme: Option<&crate::Theme>,
) -> Result<crate::Element<'static>, IconError> {
    let data = get_icon_data(icon)?;
    let is_dark = theme
        .map(|t| t.extended_palette().background.weakest.color.r < 0.5)
        .unwrap_or(true);

    let rgba = if is_dark {
        invert_rgba(&data.rgba)
    } else {
        data.rgba.clone()
    };

    let handle = Handle::from_rgba(data.width, data.height, rgba);

    Ok(Image::new(handle)
        .width(width)
        .height(height)
        .filter_method(iced_widget::image::FilterMethod::Linear)
        .into())
}
