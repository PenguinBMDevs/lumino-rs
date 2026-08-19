//! 三缓冲状态管理 - 三个槽位索引的打包/解包工具
//!
//! 将 writer/ready/reading 三个槽位索引打包进单个 `u32` 以实现原子整体置换。
//!
//! 布局：
//! ```text
//! bits 0-7:   writer slot index (0/1/2)
//! bits 8-15:  ready slot index
//! bits 16-23: reading slot index
//! bits 24-31: reserved (always 0)
//! ```

/// 三缓冲槽位索引常量
pub const WRITER: usize = 0;
pub const READY: usize = 1;
pub const READING: usize = 2;

/// 打包三个索引到 `u32`：`(reading << 16) | (ready << 8) | writer`
#[inline]
pub fn pack_state(writer: u8, ready: u8, reading: u8) -> u32 {
    (writer as u32) | ((ready as u32) << 8) | ((reading as u32) << 16)
}

/// 从打包状态中提取 writer 槽位索引
#[inline]
pub fn unpack_writer(state: u32) -> usize {
    (state & 0xFF) as usize
}

/// 从打包状态中提取 ready 槽位索引
#[inline]
pub fn unpack_ready(state: u32) -> usize {
    ((state >> 8) & 0xFF) as usize
}

/// 从打包状态中提取 reading 槽位索引
#[inline]
pub fn unpack_reading(state: u32) -> usize {
    ((state >> 16) & 0xFF) as usize
}
