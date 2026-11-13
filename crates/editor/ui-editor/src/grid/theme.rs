//! 主题颜色工具

use lumino_ui_core::Theme;

/// 主题扩展工具 trait
pub trait ThemeExt {
    /// 判断是否为亮色主题
    fn is_light(&self) -> bool;

    /// 获取键盘背景色
    fn keyboard_background_color(&self) -> lumino_ui_core::geom::Color;

    /// 获取白键颜色
    fn white_key_color(&self) -> lumino_ui_core::geom::Color;

    /// 获取黑键颜色
    fn black_key_color(&self) -> lumino_ui_core::geom::Color;

    /// 获取边框颜色
    fn border_color(&self) -> lumino_ui_core::geom::Color;

    /// 获取标尺背景色
    fn ruler_background_color(&self) -> lumino_ui_core::geom::Color;

    /// 获取文本颜色（亮色=黑，暗色=白）
    fn text_color(&self) -> lumino_ui_core::geom::Color;

    /// 获取小节线颜色
    fn bar_line_color(&self) -> lumino_ui_core::geom::Color;

    /// 获取拍线颜色
    fn beat_line_color(&self) -> lumino_ui_core::geom::Color;

    /// 获取半拍线颜色
    fn half_beat_line_color(&self) -> lumino_ui_core::geom::Color;

    /// 获取网格线颜色
    fn grid_line_color(&self) -> lumino_ui_core::geom::Color;
}

impl ThemeExt for Theme {
    fn is_light(&self) -> bool {
        if lumino_ui_core::theme::is_high_contrast() {
            return false;
        }
        self.extended_palette().background.weakest.color.r > 0.5
    }

    fn keyboard_background_color(&self) -> lumino_ui_core::geom::Color {
        if lumino_ui_core::theme::is_high_contrast() {
            return lumino_ui_core::theme::hc::KEYBOARD_BG;
        }
        let palette = self.extended_palette().background;
        if self.is_light() {
            palette.weak.color
        } else {
            palette.base.color
        }
    }

    fn white_key_color(&self) -> lumino_ui_core::geom::Color {
        if lumino_ui_core::theme::is_high_contrast() {
            return lumino_ui_core::theme::hc::WHITE_KEY;
        }
        let palette = self.extended_palette().background;
        if self.is_light() {
            palette.weak.color
        } else {
            palette.weakest.color
        }
    }

    fn black_key_color(&self) -> lumino_ui_core::geom::Color {
        if lumino_ui_core::theme::is_high_contrast() {
            return lumino_ui_core::theme::hc::BLACK_KEY;
        }
        let palette = self.extended_palette().background;
        if self.is_light() {
            palette.strong.color
        } else {
            palette.base.color
        }
    }

    fn border_color(&self) -> lumino_ui_core::geom::Color {
        if lumino_ui_core::theme::is_high_contrast() {
            return lumino_ui_core::theme::hc::BORDER;
        }
        let palette = self.extended_palette().background;
        if self.is_light() {
            palette.strongest.color
        } else {
            palette.base.color
        }
    }

    fn ruler_background_color(&self) -> lumino_ui_core::geom::Color {
        if lumino_ui_core::theme::is_high_contrast() {
            return lumino_ui_core::theme::hc::RULER_BG;
        }
        let palette = self.extended_palette().background;
        if self.is_light() {
            palette.weakest.color
        } else {
            palette.base.color
        }
    }

    fn text_color(&self) -> lumino_ui_core::geom::Color {
        if lumino_ui_core::theme::is_high_contrast() {
            return lumino_ui_core::theme::hc::TEXT;
        }
        if self.is_light() {
            lumino_ui_core::geom::Color::BLACK
        } else {
            lumino_ui_core::geom::Color::WHITE
        }
    }

    fn bar_line_color(&self) -> lumino_ui_core::geom::Color {
        if lumino_ui_core::theme::is_high_contrast() {
            return lumino_ui_core::theme::hc::BAR_LINE;
        }
        if self.is_light() {
            lumino_ui_core::geom::Color {
                a: 0.8,
                ..lumino_ui_core::geom::Color::BLACK
            }
        } else {
            lumino_ui_core::geom::Color {
                a: 0.8,
                ..lumino_ui_core::geom::Color::WHITE
            }
        }
    }

    fn beat_line_color(&self) -> lumino_ui_core::geom::Color {
        if lumino_ui_core::theme::is_high_contrast() {
            return lumino_ui_core::theme::hc::BEAT_LINE;
        }
        if self.is_light() {
            lumino_ui_core::geom::Color {
                a: 0.4,
                ..lumino_ui_core::geom::Color::BLACK
            }
        } else {
            lumino_ui_core::geom::Color {
                a: 0.4,
                ..lumino_ui_core::geom::Color::WHITE
            }
        }
    }

    fn half_beat_line_color(&self) -> lumino_ui_core::geom::Color {
        if lumino_ui_core::theme::is_high_contrast() {
            return lumino_ui_core::theme::hc::HALF_BEAT_LINE;
        }
        if self.is_light() {
            lumino_ui_core::geom::Color {
                a: 0.2,
                ..lumino_ui_core::geom::Color::BLACK
            }
        } else {
            lumino_ui_core::geom::Color {
                a: 0.2,
                ..lumino_ui_core::geom::Color::WHITE
            }
        }
    }

    fn grid_line_color(&self) -> lumino_ui_core::geom::Color {
        if lumino_ui_core::theme::is_high_contrast() {
            return lumino_ui_core::theme::hc::GRID_LINE;
        }
        if self.is_light() {
            lumino_ui_core::geom::Color {
                a: 0.1,
                ..lumino_ui_core::geom::Color::BLACK
            }
        } else {
            lumino_ui_core::geom::Color {
                a: 0.1,
                ..lumino_ui_core::geom::Color::WHITE
            }
        }
    }
}
