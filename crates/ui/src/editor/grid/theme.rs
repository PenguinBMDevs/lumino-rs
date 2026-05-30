//! 主题颜色工具

use crate::Theme;
use iced_core::Border;

/// 主题扩展工具 trait
pub trait ThemeExt {
    /// 判断是否为亮色主题
    fn is_light(&self) -> bool;

    /// 获取键盘背景色
    fn keyboard_background_color(&self) -> iced_core::Color;

    /// 获取白键颜色
    fn white_key_color(&self) -> iced_core::Color;

    /// 获取黑键颜色
    fn black_key_color(&self) -> iced_core::Color;

    /// 获取边框颜色
    fn border_color(&self) -> iced_core::Color;

    /// 获取标尺背景色
    fn ruler_background_color(&self) -> iced_core::Color;

    /// 获取文本颜色（亮色=黑，暗色=白）
    fn text_color(&self) -> iced_core::Color;

    /// 获取小节线颜色
    fn bar_line_color(&self) -> iced_core::Color;

    /// 获取拍线颜色
    fn beat_line_color(&self) -> iced_core::Color;

    /// 获取半拍线颜色
    fn half_beat_line_color(&self) -> iced_core::Color;

    /// 获取网格线颜色
    fn grid_line_color(&self) -> iced_core::Color;
}

impl ThemeExt for Theme {
    fn is_light(&self) -> bool {
        self.extended_palette().background.weakest.color.r > 0.5
    }

    fn keyboard_background_color(&self) -> iced_core::Color {
        let palette = self.extended_palette().background;
        if self.is_light() {
            palette.weak.color
        } else {
            palette.base.color
        }
    }

    fn white_key_color(&self) -> iced_core::Color {
        let palette = self.extended_palette().background;
        if self.is_light() {
            palette.weak.color
        } else {
            palette.weakest.color
        }
    }

    fn black_key_color(&self) -> iced_core::Color {
        let palette = self.extended_palette().background;
        if self.is_light() {
            palette.strong.color
        } else {
            palette.base.color
        }
    }

    fn border_color(&self) -> iced_core::Color {
        let palette = self.extended_palette().background;
        if self.is_light() {
            palette.strongest.color
        } else {
            palette.base.color
        }
    }

    fn ruler_background_color(&self) -> iced_core::Color {
        let palette = self.extended_palette().background;
        if self.is_light() {
            palette.weakest.color
        } else {
            palette.base.color
        }
    }

    fn text_color(&self) -> iced_core::Color {
        if self.is_light() {
            iced_core::Color::BLACK
        } else {
            iced_core::Color::WHITE
        }
    }

    fn bar_line_color(&self) -> iced_core::Color {
        if self.is_light() {
            iced_core::Color {
                a: 0.8,
                ..iced_core::Color::BLACK
            }
        } else {
            iced_core::Color {
                a: 0.8,
                ..iced_core::Color::WHITE
            }
        }
    }

    fn beat_line_color(&self) -> iced_core::Color {
        if self.is_light() {
            iced_core::Color {
                a: 0.4,
                ..iced_core::Color::BLACK
            }
        } else {
            iced_core::Color {
                a: 0.4,
                ..iced_core::Color::WHITE
            }
        }
    }

    fn half_beat_line_color(&self) -> iced_core::Color {
        if self.is_light() {
            iced_core::Color {
                a: 0.2,
                ..iced_core::Color::BLACK
            }
        } else {
            iced_core::Color {
                a: 0.2,
                ..iced_core::Color::WHITE
            }
        }
    }

    fn grid_line_color(&self) -> iced_core::Color {
        if self.is_light() {
            iced_core::Color {
                a: 0.1,
                ..iced_core::Color::BLACK
            }
        } else {
            iced_core::Color {
                a: 0.1,
                ..iced_core::Color::WHITE
            }
        }
    }
}

/// 创建标准边框样式
pub fn create_border_style(color: iced_core::Color) -> Border {
    Border::default().width(1.0).color(color)
}
