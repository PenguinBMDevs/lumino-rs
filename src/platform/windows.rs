use winapi::shared::minwindef::{LPARAM, LRESULT, WPARAM};
use winapi::shared::windef::{HWND, RECT};
use winapi::um::winuser::{
    DefWindowProcW, GWL_WNDPROC, GetWindowRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCLIENT,
    HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, SetWindowLongPtrW, WM_NCHITTEST,
};
use winit::window::Window;

/// 窗口过程类型
type WndProcType = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

/// 存储原始窗口过程的指针
static mut ORIGINAL_WNDPROC: Option<isize> = None;

/// 窗口边框拉伸区域宽度（像素）
const RESIZE_BORDER_WIDTH: i32 = 12;

/// 窗口过程钩子
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NCHITTEST {
        // 调用原始窗口过程获取默认结果
        let original_result = unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };

        // 如果默认结果是客户区，则检查是否在拉伸区域内
        if original_result == HTCLIENT as LRESULT {
            // 获取光标位置（屏幕坐标）
            let screen_x = (lparam & 0xFFFF) as i32;
            let screen_y = ((lparam >> 16) & 0xFFFF) as i32;

            // 获取窗口矩形
            let mut rect: RECT = unsafe { std::mem::zeroed() };
            unsafe { GetWindowRect(hwnd, &mut rect) };

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
    unsafe {
        if let Some(old_proc) = ORIGINAL_WNDPROC {
            return std::mem::transmute::<isize, WndProcType>(old_proc)(hwnd, msg, wparam, lparam);
        }
    }

    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// 为窗口设置自定义拉伸区域
pub fn setup_resize_border(window: &Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    // TODO: 非 Windows 平台编译时此函数不应被调用，应在调用处加 #[cfg(target_os = "windows")]
    unsafe {
        let handle = window.window_handle().expect("Failed to get window handle");
        let hwnd = if let RawWindowHandle::Win32(handle) = handle.as_raw() {
            handle.hwnd.get() as HWND
        } else {
            panic!("Not a Windows window");
        };

        // 获取原始窗口过程
        let original_wndproc = SetWindowLongPtrW(
            hwnd,
            GWL_WNDPROC,
            window_proc as *const () as usize as isize,
        );

        if original_wndproc != 0 {
            ORIGINAL_WNDPROC = Some(original_wndproc);
        }
    }
}
