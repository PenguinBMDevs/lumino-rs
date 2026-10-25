//! 文字工具交互与字形采样生成音符
//!
//! 交互流程（与曲线工具 / 图片转 MIDI 一致的「拉框 → 按钮确认」范式）：
//! - 选中文字工具后，在画布拖拽拉出文本框（横向：X 向吸附音符精度、Y 向吸附 key 线）；
//! - 松手进入编辑态，画布覆盖层出现 `TextInput` 供输入文字；
//! - 框右侧 √（确认生成）/ ×（取消）/ 模式按钮（正常 / key 范围合并）；
//! - 确认时按字形占位采样生成音符。
//!
//! **横向 / 纵向卷帘都"正着读"**：字形两轴挂到哪个逻辑轴由 [`GlyphGrid`] 唯一决定
//! （横向：列→tick、行→key；纵向：列→key、行→tick）。纵向视图 X = key、Y = tick，
//! 只有把字形 advance 挂到 key 轴上，文字在屏幕上才是正立的。

mod font;
mod parametric;
mod rasterize;
mod tool;

use lumino_core::TextLayoutMode;
use lumino_editor_state::text_tool::TextToolState;

pub(crate) use parametric::rasterize_text_params;
#[cfg(test)]
pub(crate) use rasterize::rasterize_glyph_alpha_with_font;
pub(crate) use rasterize::{
    GlyphGrid, rasterize_glyph_alpha, rasterize_text, sample_to_notes_grid,
};

/// 按工具当前模式分派灰度图：LegacyBox 保持原路径，Parametric 走参数化路径。
pub(crate) fn rasterize_glyph_alpha_for_tool(
    tt: &TextToolState,
    text: &str,
    cols: usize,
    rows: usize,
) -> Option<(u32, u32, Vec<u8>)> {
    match tt.layout_mode {
        TextLayoutMode::LegacyBox => rasterize_glyph_alpha(text, cols, rows, tt.font_family),
        TextLayoutMode::Parametric => {
            parametric::rasterize_glyph_alpha_params(text, cols, rows, &tt.params)
        }
    }
}

/// 按工具当前模式分派占用网格（预览与确认共用）。
pub(crate) fn rasterize_text_for_tool(
    tt: &TextToolState,
    cols: usize,
    rows: usize,
) -> Option<Vec<Vec<bool>>> {
    match tt.layout_mode {
        TextLayoutMode::LegacyBox => rasterize_text(&tt.text, cols, rows, tt.font_family),
        TextLayoutMode::Parametric => {
            parametric::rasterize_text_params(&tt.text, cols, rows, &tt.params)
        }
    }
}

#[cfg(test)]
mod tests;
