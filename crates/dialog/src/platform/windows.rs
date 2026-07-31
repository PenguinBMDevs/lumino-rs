use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winapi::shared::minwindef::{LPARAM, LRESULT, WPARAM};
use winapi::shared::windef::{HWND, RECT};
use winapi::um::winuser::{
    CallWindowProcW, DefWindowProcW, GWL_WNDPROC, GetWindowRect, HTBOTTOM, HTBOTTOMLEFT,
    HTBOTTOMRIGHT, HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, SetWindowLongPtrW,
    WM_NCHITTEST,
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

    // 光标在客户区时，检查是否在边缘拉伸区域
    let mut rect = std::mem::MaybeUninit::<RECT>::uninit();
    if unsafe { GetWindowRect(hwnd, rect.as_mut_ptr()) } == 0 {
        return original_result;
    }
    let rect = unsafe { rect.assume_init() };
    let screen_x = (lparam & 0xFFFF) as i32;
    let screen_y = ((lparam >> 16) & 0xFFFF) as i32;
    let window_x = screen_x - rect.left;
    let window_y = screen_y - rect.top;
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let left = window_x < RESIZE_BORDER_WIDTH;
    let right = window_x > width - RESIZE_BORDER_WIDTH;
    let top = window_y < RESIZE_BORDER_WIDTH;
    let bottom = window_y > height - RESIZE_BORDER_WIDTH;

    match (left, top, right, bottom) {
        (true, true, _, _) => HTTOPLEFT as LRESULT,
        (true, false, _, true) => HTBOTTOMLEFT as LRESULT,
        (false, true, true, _) => HTTOPRIGHT as LRESULT,
        (false, false, true, true) => HTBOTTOMRIGHT as LRESULT,
        (true, _, _, _) => HTLEFT as LRESULT,
        (_, true, _, _) => HTTOP as LRESULT,
        (_, _, true, _) => HTRIGHT as LRESULT,
        (_, _, _, true) => HTBOTTOM as LRESULT,
        _ => original_result,
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
