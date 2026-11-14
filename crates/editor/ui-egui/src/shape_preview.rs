//! 形状工具预览（待确认图形 + 实时拖拽）——egui 版。
//!
//! 取代 旧版 `shape_tool_box::draw_one_shape` 的可见部分：
//! - 引导轮廓：固定 1.5px、与粗细无关；
//! - 填充：主题主色 35% 透明；
//! - 描边产物预览：`shape_outline_blocks` 与 √ 写入同源，逐块画实心矩形。
//!
//! 旧版 Canvas 的绘制层被 egui 底层纹理覆盖后不可见，本模块保证预览始终可见。

use egui::{Color32, Pos2, Rect, Stroke};

use crate::theme;

/// 引导轮廓宽度（与 旧版 `OUTLINE_GUIDE_WIDTH` 一致）。
const GUIDE_WIDTH: f32 = 1.5;
/// 填充透明度（与 旧版 `FILL_ALPHA` 一致）。
const FILL_ALPHA: u8 = 89; // 0.35 * 255

/// 单个预览图形。
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewShape {
    /// 闭合多边形顶点（窗口逻辑坐标）。
    pub points: Vec<Pos2>,
    /// 是否填充（填充桶）。
    pub filled: bool,
    /// 描边产物块（窗口逻辑坐标）。
    pub blocks: Vec<Rect>,
}

/// 形状预览模型。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShapePreviewModel {
    /// 画布内容区裁剪（扣掉左/右侧栏）；`None` = 不裁剪。
    pub clip: Option<Rect>,
    /// 待确认图形 + 拖拽预览。
    pub shapes: Vec<PreviewShape>,
}

impl ShapePreviewModel {
    /// 是否没有内容。
    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }
}

/// 绘制全部预览。
pub fn show(ui: &egui::Ui, model: &ShapePreviewModel) {
    let clip = model.clip.unwrap_or_else(|| ui.clip_rect());
    let painter = ui.painter().with_clip_rect(clip);
    let base = theme::accent_strong();
    let fill = Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), FILL_ALPHA);
    for shape in &model.shapes {
        if shape.points.len() >= 3 {
            if shape.filled {
                painter.add(egui::Shape::convex_polygon(
                    shape.points.clone(),
                    fill,
                    Stroke::NONE,
                ));
            }
            let mut line = shape.points.clone();
            if let Some(first) = line.first().copied() {
                line.push(first);
            }
            painter.add(egui::Shape::line(line, Stroke::new(GUIDE_WIDTH, base)));
        }
        for block in &shape.blocks {
            if block.width() > 0.0 && block.height() > 0.0 {
                painter.rect_filled(*block, 0, base);
            }
        }
    }
}
