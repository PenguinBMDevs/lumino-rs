//! 真实 SVG 图标加载与纹理缓存。
//!
//! 图标字节与生产 `lumino-ui-core::resources::icon` 同源（`resources/icons/**`）。
//! 暗色主题下，非品牌图标按 iced `Svg::style(color)` 的语义整体染白；
//! 品牌图标（Lumino / LogoInApp / MixerActive）保留原色。

use std::collections::HashMap;
use std::path::PathBuf;

use egui::{Color32, ColorImage, TextureHandle, TextureOptions};
use thiserror::Error;

/// 图标加载 / 光栅化错误。
#[derive(Debug, Error)]
pub enum IconError {
    /// SVG 解析失败。
    #[error("SVG 解析失败（{icon:?}）: {message}")]
    Parse {
        /// 图标。
        icon: Icon,
        /// 原始错误。
        message: String,
    },
    /// 无法创建 pixmap。
    #[error("无法创建 {size}px pixmap（{icon:?}）")]
    Pixmap {
        /// 图标。
        icon: Icon,
        /// 目标边长。
        size: u32,
    },
}

macro_rules! define_icons {
    ($(($name:ident, $path:expr)),* $(,)?) => {
        /// 图标枚举（与生产资源一一对应）。
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Icon {
            $(
                #[doc = concat!("`", stringify!($name), "`")]
                $name,
            )*
        }

        /// 全部图标。
        pub const ALL_ICONS: &[Icon] = &[$(Icon::$name,)*];

        fn bytes(icon: Icon) -> &'static [u8] {
            match icon {
                $(Icon::$name => include_bytes!($path),)*
            }
        }
    };
}

define_icons! {
    (AngleRight, "../../../../resources/icons/regular/submenu-expand-indicator.svg"),
    (FolderTree, "../../../../resources/icons/regular/ui-layout.svg"),
    (Arrangement, "../../../../resources/icons/regular/arrangement-mode.svg"),
    (Gear, "../../../../resources/icons/regular/settings-general.svg"),
    (WaveForm, "../../../../resources/icons/regular/audio-automation.svg"),
    (Lumino, "../../../../resources/icons/brands/lumino-brand.svg"),
    (LogoInApp, "../../../../resources/icons/brands/app-logo.svg"),
    (WindowMin, "../../../../resources/icons/window/min.svg"),
    (WindowMax, "../../../../resources/icons/window/max.svg"),
    (WindowUnMax, "../../../../resources/icons/window/unmax.svg"),
    (WindowClose, "../../../../resources/icons/window/close.svg"),
    (Clock, "../../../../resources/icons/sidebar/conductor-track.svg"),
    (Eye, "../../../../resources/icons/sidebar/onion-skin.svg"),
    (EyeSlash, "../../../../resources/icons/sidebar/onion-skin-disabled.svg"),
    (Plus, "../../../../resources/icons/sidebar/add-track.svg"),
    (Download, "../../../../resources/icons/sidebar/export-group.svg"),
    (PlayCircle, "../../../../resources/icons/sidebar/waterfall-record.svg"),
    (EllipsisVertical, "../../../../resources/icons/sidebar/toolbar-overflow-trigger.svg"),
    (RollBarHorizontal, "../../../../resources/icons/sidebar/roll-bar-horizontal.svg"),
    (RollBarVertical, "../../../../resources/icons/sidebar/roll-bar-vertical.svg"),
    (Users, "../../../../resources/icons/toolbar/collaboration.svg"),
    (Play, "../../../../resources/icons/toolbar/playback-start.svg"),
    (Pause, "../../../../resources/icons/toolbar/playback-pause.svg"),
    (SkipBackward, "../../../../resources/icons/toolbar/playback-jump-start.svg"),
    (SkipForward, "../../../../resources/icons/toolbar/playback-jump-end.svg"),
    (Undo, "../../../../resources/icons/toolbar/history-undo.svg"),
    (Redo, "../../../../resources/icons/toolbar/history-redo.svg"),
    (MousePointer, "../../../../resources/icons/toolbar/select-tool.svg"),
    (MousePointerYSelect, "../../../../resources/icons/toolbar/y-axis-select-tool.svg"),
    (Pencil, "../../../../resources/icons/toolbar/note-draw-tool.svg"),
    (Eraser, "../../../../resources/icons/toolbar/eraser-tool.svg"),
    (Curve, "../../../../resources/icons/toolbar/curve-tool.svg"),
    (PaintBucket, "../../../../resources/icons/toolbar/paint-bucket.svg"),
    (DrawToolbox, "../../../../resources/icons/toolbar/draw-toolbox.svg"),
    (OnionSkin, "../../../../resources/icons/toolbar/onion-skin.svg"),
    (CaretDown, "../../../../resources/icons/toolbar/caret-down.svg"),
    (Thickness, "../../../../resources/icons/toolbar/stroke-settings.svg"),
    (BrushTool, "../../../../resources/icons/toolbar/brush-tool.svg"),
    (ShapeTool, "../../../../resources/icons/toolbar/shape-tool.svg"),
    (TextInput, "../../../../resources/icons/toolbar/text-input.svg"),
    (ShapeRectangle, "../../../../resources/icons/toolbar/shape-rectangle.svg"),
    (ShapeCircle, "../../../../resources/icons/toolbar/shape-circle.svg"),
    (ShapeTriangle, "../../../../resources/icons/toolbar/shape-triangle.svg"),
    (PlusCircle, "../../../../resources/icons/toolbar/plus-circle.svg"),
    (MinusCircle, "../../../../resources/icons/toolbar/minus-circle.svg"),
    (Quantize, "../../../../resources/icons/toolbar/note-quantize.svg"),
    (Speed, "../../../../resources/icons/toolbar/playback-speed.svg"),
    (FlipVertical, "../../../../resources/icons/toolbar/note-flip-vertical.svg"),
    (FlipHorizontal, "../../../../resources/icons/toolbar/note-flip-horizontal.svg"),
    (ArrowsLeftRight, "../../../../resources/icons/toolbar/loop-range-active.svg"),
    (Scroll, "../../../../resources/icons/toolbar/autoscroll-scrolling.svg"),
    (Ban, "../../../../resources/icons/toolbar/loop-range-disabled.svg"),
    (TransposeUp, "../../../../resources/icons/toolbar/note-transpose-up.svg"),
    (TransposeDown, "../../../../resources/icons/toolbar/note-transpose-down.svg"),
    (Split, "../../../../resources/icons/toolbar/note-split.svg"),
    (Glue, "../../../../resources/icons/toolbar/note-glue.svg"),
    (Tie, "../../../../resources/icons/toolbar/note-tie.svg"),
    (ImageToMidi, "../../../../resources/icons/toolbar/image-to-midi-converter.svg"),
    (MaterialLibrary, "../../../../resources/icons/toolbar/material-library.svg"),
    (PianoWaterfall, "../../../../resources/icons/sidebar/piano-waterfall.svg"),
    (PencilOutline, "../../../../resources/icons/titlebar/editor-mode.svg"),
    (Keys, "../../../../resources/icons/titlebar/piano-roll.svg"),
    (VideoCamera, "../../../../resources/icons/sidebar/video-export.svg"),
    (MusicNote, "../../../../resources/icons/sidebar/audio-export.svg"),
    (ContextMenuCut, "../../../../resources/icons/context-menu/cut-notes.svg"),
    (ContextMenuCopy, "../../../../resources/icons/context-menu/copy-notes.svg"),
    (ContextMenuPaste, "../../../../resources/icons/context-menu/paste-notes.svg"),
    (ContextMenuDelete, "../../../../resources/icons/context-menu/delete-item.svg"),
    (ContextMenuSelectAll, "../../../../resources/icons/context-menu/select-all-notes.svg"),
    (ContextMenuColorPalette, "../../../../resources/icons/context-menu/set-track-color.svg"),
    (ContextMenuChannel, "../../../../resources/icons/context-menu/set-midi-channel.svg"),
    (ContextMenuPort, "../../../../resources/icons/context-menu/set-midi-port.svg"),
    (ContextMenuRecoverTrack, "../../../../resources/icons/context-menu/recover-deleted-track.svg"),
    (ContextMenuUploadToCloud, "../../../../resources/icons/context-menu/upload-to-cloud.svg"),
    (Mixer, "../../../../resources/icons/sidebar/mixer.svg"),
    (MixerActive, "../../../../resources/icons/sidebar/mixer-active.svg"),
}

impl Icon {
    /// 是否保留原色（品牌 / 固定色图标）。
    pub fn keep_color(self) -> bool {
        matches!(self, Self::Lumino | Self::LogoInApp | Self::MixerActive)
    }

    /// 暗色主题下的默认染色。
    pub fn default_tint(self) -> Option<Color32> {
        if self.keep_color() {
            None
        } else {
            Some(Color32::WHITE)
        }
    }

    /// SVG 资源路径（用于诊断）。
    pub fn source_path(self) -> PathBuf {
        PathBuf::from(format!("resources/icons/{:?}.svg", self))
    }
}

/// 图标纹理缓存。
#[derive(Default)]
pub struct IconCache {
    textures: HashMap<Key, TextureHandle>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    icon: Icon,
    px: u32,
    tint: Option<[u8; 3]>,
}

impl IconCache {
    /// 取得图标纹理；按 `(icon, 物理像素, tint)` 缓存。
    pub fn texture(
        &mut self,
        ctx: &egui::Context,
        icon: Icon,
        size_pts: f32,
        tint: Option<Color32>,
    ) -> TextureHandle {
        let ppp = ctx.pixels_per_point();
        let px = (size_pts * ppp).round().max(1.0) as u32;
        let tint = tint.map(|color| [color.r(), color.g(), color.b()]);
        let key = Key { icon, px, tint };

        if let Some(texture) = self.textures.get(&key) {
            return texture.clone();
        }

        let image = match rasterize(icon, px, tint.map(|[r, g, b]| Color32::from_rgb(r, g, b))) {
            Ok(image) => image,
            Err(error) => {
                tracing::error!("{error}");
                ColorImage::new([1, 1], vec![Color32::TRANSPARENT])
            }
        };
        let texture = ctx.load_texture(
            format!("lumino-icon-{icon:?}-{px}-{tint:?}"),
            image,
            TextureOptions::LINEAR,
        );
        self.textures.insert(key, texture.clone());
        texture
    }

    /// 构造一个精确尺寸的 `egui::Image`。
    pub fn image(
        &mut self,
        ctx: &egui::Context,
        icon: Icon,
        size: f32,
        tint: Option<Color32>,
    ) -> egui::Image<'static> {
        let texture = self.texture(ctx, icon, size, tint);
        egui::Image::new(egui::load::SizedTexture::from_handle(&texture))
            .fit_to_exact_size(egui::vec2(size, size))
    }

    /// 以生产默认染色（暗色主题）取图标。
    pub fn default_image(
        &mut self,
        ctx: &egui::Context,
        icon: Icon,
        size: f32,
    ) -> egui::Image<'static> {
        self.image(ctx, icon, size, icon.default_tint())
    }
}

fn rasterize(icon: Icon, size: u32, tint: Option<Color32>) -> Result<ColorImage, IconError> {
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(bytes(icon), &options).map_err(|error| IconError::Parse {
        icon,
        message: error.to_string(),
    })?;

    let source = tree.size();
    let scale = (size as f32 / source.width()).min(size as f32 / source.height());
    let scaled_w = source.width() * scale;
    let scaled_h = source.height() * scale;
    let offset_x = (size as f32 - scaled_w) / 2.0;
    let offset_y = (size as f32 - scaled_h) / 2.0;

    let transform = tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, offset_x, offset_y);
    let mut pixmap = tiny_skia::Pixmap::new(size, size).ok_or(IconError::Pixmap { icon, size })?;
    pixmap.fill(tiny_skia::Color::TRANSPARENT);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let mut data = pixmap.data().to_vec();
    if let Some(tint) = tint
        && !icon.keep_color()
    {
        premultiply_tint(&mut data, tint);
    }

    Ok(ColorImage::from_rgba_premultiplied(
        [size as usize, size as usize],
        &data,
    ))
}

fn premultiply_tint(data: &mut [u8], tint: Color32) {
    let red = u16::from(tint.r());
    let green = u16::from(tint.g());
    let blue = u16::from(tint.b());
    let (pixels, _) = data.as_chunks_mut::<4>();
    for pixel in pixels {
        let alpha = u16::from(pixel[3]);
        if alpha == 0 {
            pixel.copy_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        pixel[0] = ((red * alpha) / 255) as u8;
        pixel[1] = ((green * alpha) / 255) as u8;
        pixel[2] = ((blue * alpha) / 255) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_icon_bytes_are_svg() {
        for icon in ALL_ICONS {
            let svg = bytes(*icon);
            assert!(
                svg.starts_with(b"<svg") || svg.starts_with(b"<?xml"),
                "{icon:?} 不是 SVG"
            );
        }
    }

    #[test]
    fn brand_icons_keep_color() {
        assert!(Icon::Lumino.keep_color());
        assert!(Icon::LogoInApp.keep_color());
        assert!(!Icon::Play.keep_color());
    }
}
