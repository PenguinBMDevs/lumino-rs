//! ChunkedList 单元测试（含 COW 语义与 50 万容量边界验证）
//!
//! - `util`: 共享测试工具类型与辅助函数
//! - `window`: `window_range` / `iter_window` 窗口定位与跨块迭代
//! - `insert`: 构建 / 插入 / 定位 / 分裂
//! - `remove`: 删除 / 范围查询 / 替换与清空 / 转回 Vec
//! - `cow`: COW 语义与内存回归

mod cow;
mod insert;
mod remove;
mod util;
mod window;
