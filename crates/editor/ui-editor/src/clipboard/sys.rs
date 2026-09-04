//! Windows 自定义剪贴板格式（Lumino 程序本体间 / 跨 DAW 二进制音符载体）
//!
//! 仅 Windows：用 `winapi` 注册私有格式并直接写入系统剪贴板，跨程序零拷贝传输。
//! - `LuminoMidiNotes`：Lumino 程序本体间二进制音符（`lumino_midi_model::clipboard` 编码）。
//! - `MidiPortalSequence`：Domino（TAKABO SOFT）互通格式（zlib 压缩的 `PortalSequenceData`）。
//!
//! 非 Windows 退化为文本 JSON（见 `crate::clipboard`）。

#![cfg(windows)]

use std::ptr;

use winapi::shared::minwindef::{HGLOBAL, UINT};
use winapi::um::winbase::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalFree, GlobalLock, GlobalSize, GlobalUnlock,
};
use winapi::um::winuser::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatA, SetClipboardData,
};

/// Lumino 私有剪贴板格式名（必须以 NUL 结尾，供 *A API）。
const LUMINO_NAME: &[u8] = b"LuminoMidiNotes\0";
/// Domino 互通剪贴板格式名。
const DOMINO_NAME: &[u8] = b"MidiPortalSequence\0";

/// 注册格式名，返回格式 ID（0 = 失败）。
unsafe fn register(name: &[u8]) -> UINT {
    unsafe { RegisterClipboardFormatA(name.as_ptr() as *const i8) }
}

/// 在已打开的剪贴板会话内，把一段数据写入指定格式（调用方负责 Open/Empty/Close）。
unsafe fn set_one(fmt: UINT, data: &[u8]) -> bool {
    unsafe {
        let h: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, data.len());
        if h.is_null() {
            return false;
        }
        let p = GlobalLock(h);
        if p.is_null() {
            GlobalFree(h);
            return false;
        }
        ptr::copy_nonoverlapping(data.as_ptr(), p as *mut u8, data.len());
        GlobalUnlock(h);
        let res = SetClipboardData(fmt, h);
        if res.is_null() {
            GlobalFree(h);
            false
        } else {
            true
        }
    }
}

/// 在已打开的剪贴板会话内，读取指定格式（调用方负责 Open/Close）。
unsafe fn get_one(fmt: UINT) -> Option<Vec<u8>> {
    unsafe {
        let h: HGLOBAL = GetClipboardData(fmt);
        if h.is_null() {
            return None;
        }
        let size = GlobalSize(h);
        let p = GlobalLock(h);
        if p.is_null() {
            return None;
        }
        let mut out = vec![0u8; size];
        ptr::copy_nonoverlapping(p as *const u8, out.as_mut_ptr(), size);
        GlobalUnlock(h);
        Some(out)
    }
}

/// 写多个私有二进制格式（一次会话，仅清空一次）。
///
/// 用于在同一个剪贴板里同时携带 Lumino 与 Domino 互通格式，避免后者被前一次
/// `EmptyClipboard` 清掉。
pub fn set_clipboard_binary_pair(lumino: &[u8], domino: &[u8]) -> bool {
    unsafe {
        if OpenClipboard(ptr::null_mut()) == 0 {
            return false;
        }
        if EmptyClipboard() == 0 {
            CloseClipboard();
            return false;
        }
        let f_lum = register(LUMINO_NAME);
        let f_dom = register(DOMINO_NAME);
        if f_lum == 0 || f_dom == 0 {
            CloseClipboard();
            return false;
        }
        let mut ok = true;
        if !set_one(f_lum, lumino) {
            ok = false;
        }
        if !set_one(f_dom, domino) {
            ok = false;
        }
        CloseClipboard();
        ok
    }
}

/// 将二进制音符载荷写入系统剪贴板（仅 Lumino 私有格式）。
pub fn set_clipboard_binary(data: &[u8]) -> bool {
    unsafe {
        if OpenClipboard(ptr::null_mut()) == 0 {
            return false;
        }
        if EmptyClipboard() == 0 {
            CloseClipboard();
            return false;
        }
        let fmt: UINT = register(LUMINO_NAME);
        if fmt == 0 {
            CloseClipboard();
            return false;
        }
        let r = set_one(fmt, data);
        CloseClipboard();
        r
    }
}

/// 从系统剪贴板读取 Lumino 私有二进制音符载荷（无则 None）。
pub fn get_clipboard_binary() -> Option<Vec<u8>> {
    unsafe {
        if OpenClipboard(ptr::null_mut()) == 0 {
            return None;
        }
        let fmt: UINT = register(LUMINO_NAME);
        if fmt == 0 {
            CloseClipboard();
            return None;
        }
        if IsClipboardFormatAvailable(fmt) == 0 {
            CloseClipboard();
            return None;
        }
        let r = get_one(fmt);
        CloseClipboard();
        r
    }
}

/// 从系统剪贴板读取 Domino 互通二进制载荷（无则 None）。
pub fn get_clipboard_domino() -> Option<Vec<u8>> {
    unsafe {
        if OpenClipboard(ptr::null_mut()) == 0 {
            return None;
        }
        let fmt: UINT = register(DOMINO_NAME);
        if fmt == 0 {
            CloseClipboard();
            return None;
        }
        if IsClipboardFormatAvailable(fmt) == 0 {
            CloseClipboard();
            return None;
        }
        let r = get_one(fmt);
        CloseClipboard();
        r
    }
}
