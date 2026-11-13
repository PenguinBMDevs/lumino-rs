//! 由 wgpu 离屏纹理绘制的预览区（渲染器面板 16:9 卡片 / 全屏瀑布流）。
//!
//! 纹理本体由宿主注册成 egui 外部纹理（[`RENDER_PREVIEW_TEXTURE_ID`]），
//! 本模块只负责把纹理铺到 `rects` 给出的矩形上——不持有任何 wgpu 资源。
//!
//! 两块区域**互斥**出现（全屏瀑布流模式或渲染器入口二选一），所以共用一个纹理 ID。

use egui::Rect;

/// 渲染器预览 / 全屏瀑布流共用的外部纹理 ID。
pub const RENDER_PREVIEW_TEXTURE_ID: egui::TextureId = egui::TextureId::User(0x4C55_4D49_5052);

/// 纹理预览模型。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TexturePreviewModel {
    /// 需要铺图的矩形（逻辑坐标；来自 旧版 布局探针）。
    pub rects: Vec<Rect>,
    /// 外部纹理；`None` = 门控判定不显示瀑布流，旧版 的占位文本继续生效。
    pub texture: Option<egui::TextureId>,
}
