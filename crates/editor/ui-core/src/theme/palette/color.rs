//! 颜色运算：OKLCH 明暗调整、混合与可读性。
use crate::geom::Color;

struct Oklch {
    l: f32,
    c: f32,
    h: f32,
    a: f32,
}

/// Darkens a [`Color`] by the given factor.
pub fn darken(color: Color, amount: f32) -> Color {
    let mut oklch = to_oklch(color);

    // We try to bump the chroma a bit for more colorful palettes
    if oklch.c > 0.0 && oklch.c < (1.0 - oklch.l) / 2.0 {
        // Formula empirically and cluelessly derived
        oklch.c *= 1.0 + (0.2 / oklch.c).min(100.0) * amount;
    }

    oklch.l = if oklch.l - amount < 0.0 {
        0.0
    } else {
        oklch.l - amount
    };

    from_oklch(oklch)
}

/// Lightens a [`Color`] by the given factor.
pub fn lighten(color: Color, amount: f32) -> Color {
    let mut oklch = to_oklch(color);

    // We try to bump the chroma a bit for more colorful palettes
    // Formula empirically and cluelessly derived
    oklch.c *= 1.0 + 2.0 * amount / oklch.l.max(0.05);

    oklch.l = if oklch.l + amount > 1.0 {
        1.0
    } else {
        oklch.l + amount
    };

    from_oklch(oklch)
}

/// Deviates a [`Color`] by the given factor. Lightens if the [`Color`] is
/// dark, darkens otherwise.
pub fn deviate(color: Color, amount: f32) -> Color {
    if is_dark(color) {
        lighten(color, amount)
    } else {
        darken(color, amount)
    }
}

/// Mixes two colors by the given factor.
pub fn mix(a: Color, b: Color, factor: f32) -> Color {
    let b_amount = factor.clamp(0.0, 1.0);
    let a_amount = 1.0 - b_amount;

    let a_linear = a.into_linear().map(|c| c * a_amount);
    let b_linear = b.into_linear().map(|c| c * b_amount);

    Color::from_linear_rgba(
        a_linear[0] + b_linear[0],
        a_linear[1] + b_linear[1],
        a_linear[2] + b_linear[2],
        a_linear[3] + b_linear[3],
    )
}

/// Computes a [`Color`] from the given text color that is
/// readable on top of the given background color.
pub fn readable(background: Color, text: Color) -> Color {
    if text.is_readable_on(background) {
        return text;
    }

    let improve = if is_dark(background) { lighten } else { darken };

    // TODO: Compute factor from relative contrast value
    let candidate = improve(text, 0.1);

    if candidate.is_readable_on(background) {
        return candidate;
    }

    let candidate = improve(text, 0.2);

    if candidate.is_readable_on(background) {
        return candidate;
    }

    let white_contrast = background.relative_contrast(Color::WHITE);
    let black_contrast = background.relative_contrast(Color::BLACK);

    if white_contrast >= black_contrast {
        mix(Color::WHITE, background, 0.05)
    } else {
        mix(Color::BLACK, background, 0.05)
    }
}

/// Returns true if the [`Color`] is dark.
pub fn is_dark(color: Color) -> bool {
    to_oklch(color).l < 0.6
}

// https://en.wikipedia.org/wiki/Oklab_color_space#Conversions_between_color_spaces
fn to_oklch(color: Color) -> Oklch {
    let [r, g, b, alpha] = color.into_linear();

    // linear RGB → LMS
    let l = 0.41222146 * r + 0.53633255 * g + 0.051445995 * b;
    let m = 0.2119035 * r + 0.6806995 * g + 0.10739696 * b;
    let s = 0.08830246 * r + 0.28171885 * g + 0.6299787 * b;

    // Nonlinear transform (cube root)
    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();

    // LMS → Oklab
    let l = 0.21045426 * l_ + 0.7936178 * m_ - 0.004072047 * s_;
    let a = 1.9779985 * l_ - 2.4285922 * m_ + 0.4505937 * s_;
    let b = 0.025904037 * l_ + 0.78277177 * m_ - 0.80867577 * s_;

    // Oklab → Oklch
    let c = (a * a + b * b).sqrt();
    let h = b.atan2(a); // radians

    Oklch { l, c, h, a: alpha }
}

// https://en.wikipedia.org/wiki/Oklab_color_space#Conversions_between_color_spaces
fn from_oklch(oklch: Oklch) -> Color {
    let Oklch { l, c, h, a: alpha } = oklch;

    let a = c * h.cos();
    let b = c * h.sin();

    // Oklab → LMS (nonlinear)
    let l_ = l + 0.39633778 * a + 0.21580376 * b;
    let m_ = l - 0.105561346 * a - 0.06385417 * b;
    let s_ = l - 0.08948418 * a - 1.2914855 * b;

    // Cubing back
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    let r = 4.0767417 * l - 3.3077116 * m + 0.23096994 * s;
    let g = -1.268438 * l + 2.6097574 * m - 0.34131938 * s;
    let b = -0.0041960863 * l - 0.7034186 * m + 1.7076147 * s;

    Color::from_linear_rgba(
        r.clamp(0.0, 1.0),
        g.clamp(0.0, 1.0),
        b.clamp(0.0, 1.0),
        alpha,
    )
}
