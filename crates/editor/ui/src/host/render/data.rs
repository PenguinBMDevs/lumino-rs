use lumino_ui_core::geom::Size;

/// 渲染数据
pub struct RenderData {
    pub scroll: (f32, f32),
    pub zoom: (f32, f32),
    pub viewport_size: Size,
    pub ruler_instances: Vec<lumino_gfx::RulerTickInstance>,
    /// 走带视图覆盖层实例（背景/lane/网格/框选/指示线），每帧重建
    pub arrangement_overlay_instances: Vec<lumino_gfx::ArrangementNoteInstance>,
    /// 覆盖层中"背景层"实例数（背景/lane/网格），绘制在音符之下
    pub arrangement_overlay_back_len: usize,
    /// 走带视图侧栏音轨顺序（文档音轨 id 列表，索引=泳道序号）。
    ///
    /// 走带音符直接复用钢琴卷帘常驻 GPU 音符缓冲（零第二份显存），
    /// 该顺序用于把文档音轨映射到泳道序号，由渲染线程段表定位可见音符。
    pub arrangement_track_order: Vec<usize>,
    /// 走带视图各泳道可见性（与 `arrangement_track_order` 对齐，`false`=静音/隐藏不绘制）。
    pub arrangement_track_visible: Vec<bool>,
    pub cc_bar_instances: Vec<lumino_gfx::CcBarInstance>,
    /// CPU 洋葱皮叠加帧（`None` = 不绘制；默认 GPU 后端恒为 `None`）。
    ///
    /// 由 [`crate::host::render::cpu_onion`] 每帧生成/复用：像素为透明背景 RGBA8
    /// sRGB，矩形为**物理像素**；`version` 变化时渲染线程才重传纹理。
    pub cpu_onion_frame: Option<lumino_gfx::CpuOnionFrame>,
}

/// 视口信息
pub struct ViewportInfo {
    pub canvas_offset: lumino_ui_core::geom::Point,
    pub canvas_size: lumino_ui_core::geom::Point,
}

/// 从主题提取的网格渲染颜色。
///
/// **全部为线性空间分量**（不是主题色板里的 sRGB 值）。原因见
/// [`GridColors::from_theme`] 的文档。
pub struct GridColors {
    pub bg: [f32; 4],
    pub black_key: [f32; 4],
    pub bar_line: [f32; 4],
    pub beat_line: [f32; 4],
    pub half_beat_line: [f32; 4],
    pub grid_line: [f32; 4],
    pub key_line: [f32; 4],
}

impl GridColors {
    /// 从主题提取网格渲染颜色，并转换到**线性空间**。
    ///
    /// # 为什么必须在这里转线性（网格背景"太亮"的根因）
    ///
    /// gfx 离屏目标是 sRGB 纹理（`Bgra8UnormSrgb`，见
    /// `gfx::render_thread::render_loop::textures`）：着色器输出被硬件按
    /// 「线性 → sRGB」编码一次；egui 采样后再按 sRGB 解码一次
    /// （`egui-bridge::egui.wgsl` 的 `fs_main_srgb`），净变换是恒等。
    ///
    /// 因此 uniform 里必须放**线性**值，屏幕像素才等于主题色板值。若直接塞
    /// 主题的 sRGB 分量，画面会被额外编码一次：主题 `#24283B` 的卷帘背景在屏
    /// 上变成亮灰 `#696E84`（8 位值约为原值的 2.9 倍），即"网格背景比 UI 背景
    /// 亮一截、长时间看晃眼"的直接原因。
    ///
    /// 口径与 `tests/gpu-baseline/grid_renderer.png` 一致：该基线同样以线性值
    /// 喂 uniform，最终背景像素 `#22273D` ≈ 主题 `#24283B`；若直接传主题的
    /// sRGB 分量，同一路径会得到亮灰 `#696E84`。
    pub fn from_theme(theme: &crate::Theme) -> Self {
        use crate::editor::grid::theme::ThemeExt;
        let palette = theme.extended_palette().background;
        let key_line = if theme.is_light() {
            palette.strong.color
        } else {
            palette.weak.color
        };

        Self {
            bg: theme.keyboard_background_color().into_linear(),
            black_key: theme.black_key_color().into_linear(),
            bar_line: theme.bar_line_color().into_linear(),
            beat_line: theme.beat_line_color().into_linear(),
            half_beat_line: theme.half_beat_line_color().into_linear(),
            grid_line: theme.grid_line_color().into_linear(),
            key_line: key_line.into_linear(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GridColors;
    use crate::editor::grid::theme::ThemeExt;
    use lumino_ui_core::geom::Color;
    use lumino_ui_core::theme::Theme;

    /// 线性化后再编码回 sRGB，必须与主题色板逐字节等价（允许浮点误差）。
    ///
    /// 这是「屏幕像素 == 主题色」的守卫：一旦有人把这里的 `into_linear()`
    /// 去掉，网格背景会立刻整体提亮（深色主题尤其明显）。
    #[test]
    fn grid_colors_round_trip_to_theme_palette() {
        for theme in Theme::ALL {
            let colors = GridColors::from_theme(theme);
            let expected = theme.keyboard_background_color();
            let encoded = Color::from_linear_rgba(colors.bg[0], colors.bg[1], colors.bg[2], 1.0);

            for (got, want, channel) in [
                (encoded.r, expected.r, "r"),
                (encoded.g, expected.g, "g"),
                (encoded.b, expected.b, "b"),
            ] {
                assert!(
                    (got - want).abs() < 1e-4,
                    "{} 主题背景 {channel} 通道线性化往返失真：{got} vs {want}",
                    theme.name()
                );
            }
        }
    }

    /// 深色主题的网格背景必须是"暗"的（线性分量远小于 sRGB 分量），
    /// 否则就是漏了 `into_linear`（典型症状：`#24283B` 渲染成 `#696E84`）。
    #[test]
    fn dark_theme_grid_background_is_linearized() {
        let colors = GridColors::from_theme(&Theme::TokyoNightStorm);
        let srgb = Theme::TokyoNightStorm.keyboard_background_color();

        assert!(
            colors.bg[0] < srgb.r * 0.5,
            "深色主题背景线性分量应明显小于 sRGB 分量（bg.r={} vs srgb.r={}）",
            colors.bg[0],
            srgb.r
        );
        // 线性化后仍应保持"深色"：0x24 的线性值约 0.017。
        assert!(
            colors.bg[0] < 0.05,
            "Tokyo Night Storm 背景线性值应 < 0.05，实际 {}",
            colors.bg[0]
        );
    }

    /// 亮色主题同样要线性化，且不得被误判成暗色（线性值仍应 > 0.5）。
    #[test]
    fn light_theme_grid_background_stays_light() {
        let colors = GridColors::from_theme(&Theme::Light);
        assert!(
            colors.bg[0] > 0.5,
            "Light 主题背景线性值应仍为亮色，实际 {}",
            colors.bg[0]
        );
    }
}
