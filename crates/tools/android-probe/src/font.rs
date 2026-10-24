//! 内置字体注册。
//!
//! 字体文件是 Noto Sans SC（OFL-1.1）按 GB2312 字符集 + ASCII/Latin-1 + CJK 标点
//! 子集化后的静态 Regular/Bold（见 `resources/fonts/NOTICE.md`）。
//!
//! 为什么要内嵌而不是只靠系统字体：fontdb 0.23 的 `load_system_fonts()` 对 android
//! 显式跳过，iced 的 `Family::SansSerif` 在 android 上会解析不到任何字体。fork 的
//! `graphics/src/text.rs` 已补 `/system/fonts` 扫描作兜底，但「首屏一定有中文字形」
//! 必须靠内嵌字体保证。

use std::borrow::Cow;

use iced_wgpu::graphics::text::font_system;
use iced_winit::core::Font;

/// 字体家族名（与 TTF 的 `name` 表一致，`Font::with_name` 按此查找）。
pub const FAMILY: &str = "Noto Sans SC";

/// 子集化 Noto Sans SC Regular（GB2312 + ASCII/Latin-1 + CJK 标点，约 2.2 MB）。
static REGULAR: &[u8] = include_bytes!("../../../../resources/fonts/NotoSansSC-Regular.ttf");

/// 子集化 Noto Sans SC Bold（同上字符集）。
static BOLD: &[u8] = include_bytes!("../../../../resources/fonts/NotoSansSC-Bold.ttf");

/// 注册内置字体并返回 UI 字体句柄（幂等：重复注册同一份字节无副作用）。
pub fn register() -> Font {
    {
        let mut system = font_system().write().expect("字体系统锁被毒化");
        system.load_font(Cow::Borrowed(REGULAR));
        system.load_font(Cow::Borrowed(BOLD));
    }

    log::info!(
        "已注册内置字体 {FAMILY}（Regular {} KB / Bold {} KB）",
        REGULAR.len() / 1024,
        BOLD.len() / 1024
    );

    Font::with_name(FAMILY)
}
