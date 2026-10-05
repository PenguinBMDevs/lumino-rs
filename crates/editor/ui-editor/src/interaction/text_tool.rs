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
mod rasterize;
mod tool;

pub(crate) use rasterize::{
    GlyphGrid, rasterize_glyph_alpha, rasterize_text, sample_to_notes_grid,
};

#[cfg(test)]
mod tests;
