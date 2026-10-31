//! 形状抽屉画布（DRAW-033B1）
//!
//! `doc` 是不依赖 iced 的纯状态，`canvas` 只做屏幕映射、六工具交互与绘制。

/// 抽屉 Canvas Widget
pub mod canvas;
/// 抽屉文档、草稿与工具
pub mod doc;
/// 插入比例适配
pub mod fit;
