//! Windows 自定义剪贴板格式（Lumino 程序本体间二进制音符载体）
//!
//! 仅 Windows：用 `winapi` 注册私有格式 `LuminoMidiNotes`，将紧凑二进制剪贴板载荷
//! （`lumino_midi_model::clipboard` 编码）直接写入系统剪贴板，跨 Lumino 实例零拷贝传输。
//! 非 Windows 退化为文本 JSON（见 `crate::clipboard`）。

#![cfg(windows)]

use std::ptr;

use winapi::shared::minwindef::{HGLOBAL, UINT};
use winapi::um::winbase::{GlobalAlloc, GlobalFree, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use winapi::um::winuser::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatA, SetClipboardData,
};

/// 私有剪贴板格式名（必须以 NUL 结尾，供 *A API）
const FORMAT_NAME: &[u8] = b"LuminoMidiNotes\0";

/// 将二进制音符载荷写入系统剪贴板（私有格式）。
pub fn set_clipboard_binary(data: &[u8]) -> bool {
    unsafe {
        if OpenClipboard(ptr::null_mut()) == 0 {
            return false;
        }
        if EmptyClipboard() == 0 {
            CloseClipboard();
            return false;
        }
        let fmt: UINT = RegisterClipboardFormatA(FORMAT_NAME.as_ptr() as *const i8);
        if fmt == 0 {
            CloseClipboard();
            return false;
        }
        let h: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, data.len());
        if h.is_null() {
            CloseClipboard();
            return false;
        }
        let p = GlobalLock(h);
        if p.is_null() {
            GlobalFree(h);
            CloseClipboard();
            return false;
        }
        ptr::copy_nonoverlapping(data.as_ptr(), p as *mut u8, data.len());
        GlobalUnlock(h);
        let res = SetClipboardData(fmt, h);
        CloseClipboard();
        if res.is_null() {
            GlobalFree(h);
            false
        } else {
            true
        }
    }
}

/// 从系统剪贴板读取 Lumino 私有二进制音符载荷（无则 None）。
pub fn get_clipboard_binary() -> Option<Vec<u8>> {
    unsafe {
        if OpenClipboard(ptr::null_mut()) == 0 {
            return None;
        }
        let fmt: UINT = RegisterClipboardFormatA(FORMAT_NAME.as_ptr() as *const i8);
        if fmt == 0 {
            CloseClipboard();
            return None;
        }
        if IsClipboardFormatAvailable(fmt) == 0 {
            CloseClipboard();
            return None;
        }
        let h: HGLOBAL = GetClipboardData(fmt);
        if h.is_null() {
            CloseClipboard();
            return None;
        }
        let size = GlobalSize(h);
        let p = GlobalLock(h);
        if p.is_null() {
            CloseClipboard();
            return None;
        }
        let mut out = vec![0u8; size];
        ptr::copy_nonoverlapping(p as *const u8, out.as_mut_ptr(), size);
        GlobalUnlock(h);
        CloseClipboard();
        Some(out)
    }
}
