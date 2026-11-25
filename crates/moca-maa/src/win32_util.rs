#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::{CloseHandle, HWND, RECT};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, GetWindowThreadProcessId};

    pub fn is_current_process_elevated() -> bool {
        unsafe {
            let mut token = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return false;
            }
            let result = query_token_elevation(token);
            CloseHandle(token);
            result.unwrap_or(false)
        }
    }

    pub fn is_window_process_elevated(hwnd: isize) -> Option<bool> {
        unsafe {
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd as HWND, &mut pid);
            if pid == 0 {
                return None;
            }
            let proc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if proc.is_null() {
                // Access denied typically means the target process runs with higher integrity (elevated).
                return Some(true);
            }
            let mut token = std::ptr::null_mut();
            let elevated = if OpenProcessToken(proc, TOKEN_QUERY, &mut token) != 0 {
                let val = query_token_elevation(token);
                CloseHandle(token);
                val
            } else {
                Some(true)
            };
            CloseHandle(proc);
            elevated
        }
    }

    pub fn get_client_size(hwnd: isize) -> Option<(i32, i32)> {
        unsafe {
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            if GetClientRect(hwnd as HWND, &mut rect) == 0 {
                return None;
            }
            Some((rect.right - rect.left, rect.bottom - rect.top))
        }
    }

    unsafe fn query_token_elevation(token: windows_sys::Win32::Foundation::HANDLE) -> Option<bool> {
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut ret_len: u32 = 0;
        let ok = unsafe {
            GetTokenInformation(
                token,
                TokenElevation,
                (&raw mut elevation).cast(),
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut ret_len,
            )
        };
        if ok != 0 {
            Some(elevation.TokenIsElevated != 0)
        } else {
            None
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn is_current_process_elevated() -> bool {
        false
    }

    pub fn is_window_process_elevated(_hwnd: isize) -> Option<bool> {
        None
    }

    pub fn get_client_size(_hwnd: isize) -> Option<(i32, i32)> {
        None
    }
}

pub use imp::{get_client_size, is_current_process_elevated, is_window_process_elevated};
