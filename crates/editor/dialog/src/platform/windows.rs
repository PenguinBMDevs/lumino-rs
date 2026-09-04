//! **DRI（单人负责）**: lumino-editor/dialog 平台负责人。所有 Win32 `unsafe`/`transmute` 变更须经其 review。
//!
//! 安全不变量：`window_proc` 的 `transmute` 仅在窗口子类化生命周期内有效；
//! 原始窗口过程在 `SetWindowLongPtrW` 恢复前必须保持存活，否则回调悬垂。
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winapi::shared::minwindef::{LPARAM, LRESULT, WPARAM};
use winapi::shared::windef::{HWND, RECT};
use winapi::um::winuser::{
    CallWindowProcW, DefWindowProcW, GWL_WNDPROC, GetWindowRect, HTBOTTOM, HTBOTTOMLEFT,
    HTBOTTOMRIGHT, HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, IsZoomed,
    SetWindowLongPtrW, WM_NCHITTEST,
};
use winit::window::Window;

type WndProc = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

/// 窗口边框拉伸区域宽度（像素）
const RESIZE_BORDER_WIDTH: i32 = 12;

/// 按 HWND 保存原始窗口过程，避免多个窗口覆盖同一全局指针。
static ORIGINAL_WNDPROCS: OnceLock<Mutex<HashMap<usize, isize>>> = OnceLock::new();

fn original_wndprocs() -> &'static Mutex<HashMap<usize, isize>> {
    ORIGINAL_WNDPROCS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 从 `WM_NCHITTEST` 的 `lParam` 解析屏幕坐标（`GET_X_LPARAM`/`GET_Y_LPARAM` 语义）。
///
/// 低 16 位为 X、高 16 位为 Y，均为**有符号** `i16`。必须先转 `i16` 再扩到 `i32`
/// 做符号扩展，否则副屏负坐标（如左侧副屏 `x = -1920`）会被误解析为 `63536`，
/// 导致 `window_x/window_y` 巨大 → 全窗口误判为拉伸区。
fn nchittest_screen_coords(lparam: LPARAM) -> (i32, i32) {
    let packed = lparam as i32;
    let screen_x = (packed & 0xFFFF) as i16 as i32;
    let screen_y = ((packed >> 16) & 0xFFFF) as i16 as i32;
    (screen_x, screen_y)
}

/// 根据窗口内相对坐标判定拉伸命中，返回 Win32 `HT*` 码；命中客户区返回 `None`。
fn hit_test_resize_border(
    window_x: i32,
    window_y: i32,
    width: i32,
    height: i32,
) -> Option<LRESULT> {
    let left = window_x < RESIZE_BORDER_WIDTH;
    let right = window_x > width - RESIZE_BORDER_WIDTH;
    let top = window_y < RESIZE_BORDER_WIDTH;
    let bottom = window_y > height - RESIZE_BORDER_WIDTH;

    match (left, top, right, bottom) {
        (true, true, _, _) => Some(HTTOPLEFT as LRESULT),
        (true, false, _, true) => Some(HTBOTTOMLEFT as LRESULT),
        (false, true, true, _) => Some(HTTOPRIGHT as LRESULT),
        (false, false, true, true) => Some(HTBOTTOMRIGHT as LRESULT),
        (true, _, _, _) => Some(HTLEFT as LRESULT),
        (_, true, _, _) => Some(HTTOP as LRESULT),
        (_, _, true, _) => Some(HTRIGHT as LRESULT),
        (_, _, _, true) => Some(HTBOTTOM as LRESULT),
        _ => None,
    }
}

/// 窗口过程钩子（按 HWND 查找对应原始过程）
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let original = original_wndprocs()
        .lock()
        .ok()
        .and_then(|procs| procs.get(&(hwnd as usize)).copied());
    let original_proc =
        original.map(|value| unsafe { std::mem::transmute::<isize, WndProc>(value) });

    let default_result = || match original_proc {
        Some(proc) => unsafe { CallWindowProcW(Some(proc), hwnd, msg, wparam, lparam) },
        None => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    };

    if msg != WM_NCHITTEST {
        return default_result();
    }

    let original_result = default_result();
    if original_result != HTCLIENT as LRESULT {
        return original_result;
    }

    // 最大化时不提供拉伸命中，避免全屏/最大化边缘误触拉伸。
    if unsafe { IsZoomed(hwnd) } != 0 {
        return original_result;
    }

    // 光标在客户区时，检查是否在边缘拉伸区域
    let mut rect = std::mem::MaybeUninit::<RECT>::uninit();
    if unsafe { GetWindowRect(hwnd, rect.as_mut_ptr()) } == 0 {
        return original_result;
    }
    let rect = unsafe { rect.assume_init() };
    let (screen_x, screen_y) = nchittest_screen_coords(lparam);
    let window_x = screen_x - rect.left;
    let window_y = screen_y - rect.top;
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;

    if let Some(hit) = hit_test_resize_border(window_x, window_y, width, height) {
        hit
    } else {
        original_result
    }
}

/// 为指定窗口安装独立的拉伸命中测试，按 HWND 保存原始过程。
pub fn setup_resize_border(window: &Window) -> Result<(), String> {
    let handle = window
        .window_handle()
        .map_err(|e| format!("获取窗口句柄失败: {e}"))?;
    let hwnd = match handle.as_raw() {
        RawWindowHandle::Win32(handle) => handle.hwnd.get() as HWND,
        _ => return Err("不是 Windows 窗口".to_string()),
    };

    let original = unsafe {
        SetWindowLongPtrW(
            hwnd,
            GWL_WNDPROC,
            window_proc as *const () as usize as isize,
        )
    };
    if original == 0 {
        return Err("设置窗口过程失败".to_string());
    }
    original_wndprocs()
        .lock()
        .map_err(|_| "窗口过程表锁已损坏".to_string())?
        .insert(hwnd as usize, original);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 按 Win32 规则打包 `WM_NCHITTEST` 的 `lParam`（低 16 位 X，高 16 位 Y）。
    fn pack_lparam(x: i16, y: i16) -> LPARAM {
        ((u16::from_ne_bytes(x.to_ne_bytes()) as u32)
            | ((u16::from_ne_bytes(y.to_ne_bytes()) as u32) << 16)) as i32 as LPARAM
    }

    #[test]
    fn test_nchittest_coords_primary_positive() {
        let (x, y) = nchittest_screen_coords(pack_lparam(100, 200));
        assert_eq!((x, y), (100, 200));
    }

    #[test]
    fn test_nchittest_coords_secondary_negative() {
        // 左侧副屏典型负坐标：旧实现会误解析为 63536/65336。
        let (x, y) = nchittest_screen_coords(pack_lparam(-1920, -300));
        assert_eq!((x, y), (-1920, -300));
    }

    #[test]
    fn test_nchittest_coords_boundary_sign() {
        // 32767 仍为正，-32768 为最小负值，验证符号位边界。
        assert_eq!(
            nchittest_screen_coords(pack_lparam(32767, 32767)),
            (32767, 32767)
        );
        assert_eq!(
            nchittest_screen_coords(pack_lparam(-32768, -32768)),
            (-32768, -32768)
        );
    }

    #[test]
    fn test_secondary_screen_center_is_client() {
        // 回归测试：左侧副屏窗口（left=-1920）中心点不得误判为拉伸区。
        let rect_left = -1920;
        let rect_top = 100;
        let width = 1280;
        let height = 800;
        let (screen_x, screen_y) = nchittest_screen_coords(pack_lparam(-1280, 500));
        let window_x = screen_x - rect_left;
        let window_y = screen_y - rect_top;
        assert_eq!((window_x, window_y), (640, 400));
        assert_eq!(
            hit_test_resize_border(window_x, window_y, width, height),
            None
        );
    }

    #[test]
    fn test_secondary_screen_edge_still_resizes() {
        // 副屏边缘仍需命中拉伸：窗口左边缘 + 右下角。
        assert_eq!(
            hit_test_resize_border(5, 400, 1280, 800),
            Some(HTLEFT as LRESULT)
        );
        assert_eq!(
            hit_test_resize_border(1275, 795, 1280, 800),
            Some(HTBOTTOMRIGHT as LRESULT)
        );
    }
}
