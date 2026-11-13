//! 标题栏模式切换：**只保留 AppMode**。
//!
//! 旧版 侧的模式切换视图（`view`）已无调用者，随 旧版 标题栏一起删除；
//! 模式切换入口由 egui 标题栏 / 菜单提供。这里仅再导出生产 `AppMode`。

pub use lumino_ui_core::app_mode::AppMode;
