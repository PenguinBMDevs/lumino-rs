//! 工程走带剪贴板操作（复制/粘贴/剪切，Lumino 程序本体间同步）
//!
//! 使用与钢琴卷帘相同的 JSON 剪贴板格式，额外包含 origin_track；
//! 载荷携带 `division`（源 PPQN），粘贴时若与目标文档 PPQN 不一致则按 ratio 重采样，
//! 保证跨 Lumino 进程粘贴出的音符长度与数据完全一致。
//!
//! # 子模块
//! - `copy`: 复制/剪切与 JSON 剪贴板写入
//! - `encode`: 走带二进制剪贴板编码（紧凑二进制路径）
//! - `paste`: 粘贴载荷解析与批量落轨
//! - `tests`: 单元测试

/// 走带二进制剪贴板子格式哨兵（写入 `ClipRecord` 头的 `track_hint` 字段），
/// 与钢琴卷帘二进制（track_hint=0）区分，避免跨路径互相误读。
/// 非 Windows 构建时二进制读写仅被单测使用（`#[cfg(windows)]` 调用点被裁剪），
/// 用 `allow(dead_code)` 避免 `-D warnings` 下的误报；Windows 下仍正常 lint。
#[cfg_attr(not(windows), allow(dead_code))]
const ARRANGEMENT_BINARY_MARK: u16 = 0xFFFF;

mod copy;
mod encode;
mod paste;

#[cfg(test)]
mod tests;
