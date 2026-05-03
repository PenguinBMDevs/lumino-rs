use std::sync::atomic::{AtomicIsize, Ordering};

use winapi::shared::minwindef::{LPARAM, LRESULT, WPARAM};
use winapi::shared::windef::{HWND, RECT};
use winapi::um::winuser::{
    CallWindowProcW, DefWindowProcW, GWL_WNDPROC, GetWindowRect, HTBOTTOM, HTBOTTOMLEFT,
    HTBOTTOMRIGHT, HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, SetWindowLongPtrW,
    WM_NCHITTEST,
};
use winit::window::Window;

/// 窗口过程类型
type WndProcType = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

/// 存储原始窗口过程的指针
static ORIGINAL_WNDPROC: AtomicIsize = AtomicIsize::new(0);

/// 窗口边框拉伸区域宽度（像素）
const RESIZE_BORDER_WIDTH: i32 = 12;

/// 窗口过程钩子
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let old_proc_val = ORIGINAL_WNDPROC.load(Ordering::Relaxed);
    let old_proc = if old_proc_val != 0 {
        #[allow(unsafe_op_in_unsafe_fn)]
        Some(std::mem::transmute::<isize, WndProcType>(old_proc_val))
    } else {
        None
    };

    if msg == WM_NCHITTEST {
        // 调用原始窗口过程获取默认结果
        let original_result = if let Some(proc) = old_proc {
            unsafe { CallWindowProcW(Some(proc), hwnd, msg, wparam, lparam) }
        } else {
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        };

        // 如果默认结果是客户区，则检查是否在拉伸区域内
        if original_result == HTCLIENT as LRESULT {
            // 获取光标位置（屏幕坐标）
            let screen_x = (lparam & 0xFFFF) as i32;
            let screen_y = ((lparam >> 16) & 0xFFFF) as i32;

            // 获取窗口矩形（检查返回值，GetWindowRect 返回 0 表示失败）
            let mut rect = std::mem::MaybeUninit::<RECT>::uninit();
            let success = unsafe { GetWindowRect(hwnd, rect.as_mut_ptr()) };
            if success == 0 {
                // 获取窗口矩形失败，回退到原始结果
                return original_result;
            }
            let rect = unsafe { rect.assume_init() };

            // 将屏幕坐标转换为窗口坐标
            let x = screen_x - rect.left;
            let y = screen_y - rect.top;
            let window_width = rect.right - rect.left;
            let window_height = rect.bottom - rect.top;

            // 检查是否在边缘区域内
            let left = x < RESIZE_BORDER_WIDTH;
            let right = x > window_width - RESIZE_BORDER_WIDTH;
            let top = y < RESIZE_BORDER_WIDTH;
            let bottom = y > window_height - RESIZE_BORDER_WIDTH;

            // 返回对应的拉伸命中测试代码
            let hit_test_code = match (left, top, right, bottom) {
                (true, true, _, _) => HTTOPLEFT,
                (true, false, _, true) => HTBOTTOMLEFT,
                (false, true, true, _) => HTTOPRIGHT,
                (false, false, true, true) => HTBOTTOMRIGHT,
                (true, _, _, _) => HTLEFT,
                (_, true, _, _) => HTTOP,
                (_, _, true, _) => HTRIGHT,
                (_, _, _, true) => HTBOTTOM,
                _ => HTCLIENT,
            };

            if hit_test_code != HTCLIENT {
                return hit_test_code as LRESULT;
            }
        }

        return original_result;
    }

    // 调用原始窗口过程
    if let Some(proc) = old_proc {
        unsafe { CallWindowProcW(Some(proc), hwnd, msg, wparam, lparam) }
    } else {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }
}

/// 为窗口设置自定义拉伸区域
pub fn setup_resize_border(window: &Window) -> Result<(), String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    unsafe {
        let handle = window
            .window_handle()
            .map_err(|e| format!("Failed to get window handle: {}", e))?;
        let hwnd = if let RawWindowHandle::Win32(handle) = handle.as_raw() {
            handle.hwnd.get() as HWND
        } else {
            return Err("Not a Windows window".to_string());
        };

        // 获取原始窗口过程并设置新的窗口过程
        let original_wndproc = SetWindowLongPtrW(
            hwnd,
            GWL_WNDPROC,
            window_proc as *const () as usize as isize,
        );

        if original_wndproc != 0 {
            // 使用 AtomicIsize 保存，允许被新窗口的 wndproc 覆盖（虽然 winit 一般共享一个 wndproc）
            ORIGINAL_WNDPROC.store(original_wndproc, Ordering::Relaxed);
        }

        Ok(())
    }
}
