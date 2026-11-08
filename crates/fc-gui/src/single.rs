//! 单实例：第二次启动只把已有窗口叫到前面，然后自己退出。
//!
//! 不用第三方 crate：Windows 下用命名互斥体判断"是不是已经有实例"，
//! 再按窗口标题找主窗口、恢复并置前。非 Windows 直接放行。

#[cfg(windows)]
pub fn already_running() -> bool {
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    let name: Vec<u16> = "Local\\ToneBox-single-instance\0".encode_utf16().collect();
    unsafe {
        let h = CreateMutexW(null_mut(), 0, name.as_ptr());
        if h.is_null() {
            return false;
        }
        GetLastError() == ERROR_ALREADY_EXISTS
    }
}

#[cfg(not(windows))]
pub fn already_running() -> bool {
    false
}

/// 把已在运行的窗口恢复并置前
#[cfg(windows)]
pub fn activate_existing() {
    use std::ptr::null;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };

    let title: Vec<u16> = "ToneBox\0".encode_utf16().collect();
    unsafe {
        let hwnd = FindWindowW(null(), title.as_ptr());
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
pub fn activate_existing() {}
