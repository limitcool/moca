#[cfg(windows)]
mod imp {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    use windows_sys::Win32::Foundation::{CloseHandle, HWND, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC,
        GetDIBits, GetObjectW, ReleaseDC,
    };
    use windows_sys::Win32::Security::{
        GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::Shell::{ExtractIconExW, ShellExecuteW};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DestroyIcon, GCLP_HICON, GCLP_HICONSM, GetClassLongPtrW, GetClientRect, GetIconInfo,
        GetWindowThreadProcessId, HICON, ICON_BIG, ICON_SMALL, ICON_SMALL2, ICONINFO,
        SMTO_ABORTIFHUNG, SW_SHOWNORMAL, SendMessageTimeoutW, WM_GETICON,
    };

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

    /// Relaunch the current executable with Administrator privileges via UAC (`ShellExecuteW("runas")`).
    ///
    /// Returns `true` if the elevated child process was successfully started.
    pub fn relaunch_as_admin() -> bool {
        use std::os::windows::ffi::OsStrExt;

        let Ok(exe) = std::env::current_exe() else {
            return false;
        };
        let to_wide = |s: &std::ffi::OsStr| -> Vec<u16> {
            s.encode_wide().chain(std::iter::once(0)).collect()
        };

        let op: Vec<u16> = "runas\0".encode_utf16().collect();
        let file = to_wide(exe.as_os_str());

        let args_joined = std::env::args_os()
            .skip(1)
            .map(|arg| {
                let s = arg.to_string_lossy();
                if s.contains(' ') || s.is_empty() {
                    format!("\"{}\"", s.replace('"', "\\\""))
                } else {
                    s.into_owned()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let params = (!args_joined.is_empty()).then(|| to_wide(std::ffi::OsStr::new(&args_joined)));

        let cwd = std::env::current_dir().ok().map(|p| to_wide(p.as_os_str()));

        let res = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                op.as_ptr(),
                file.as_ptr(),
                params.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
                cwd.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
                SW_SHOWNORMAL,
            )
        };
        (res as usize) > 32
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

    /// Extract a window's application icon as `(width, height, rgba_bytes)` scaled down to <= 32px.
    pub fn get_window_icon_rgba(hwnd: isize) -> Option<(u32, u32, Vec<u8>)> {
        unsafe {
            let h_wnd = hwnd as HWND;
            let mut owned_icon: Option<HICON> = None;
            let mut icon = query_window_hicon(h_wnd);

            if icon.is_null()
                && let Some(exe_icon) = extract_exe_hicon_for_window(h_wnd)
            {
                icon = exe_icon;
                owned_icon = Some(exe_icon);
            }

            if icon.is_null() {
                return None;
            }

            let rgba = hicon_to_rgba(icon);
            if let Some(owned) = owned_icon {
                DestroyIcon(owned);
            }
            rgba
        }
    }

    unsafe fn query_window_hicon(hwnd: HWND) -> HICON {
        for icon_type in [ICON_SMALL2, ICON_SMALL, ICON_BIG] {
            let mut result: usize = 0;
            let ok = unsafe {
                SendMessageTimeoutW(
                    hwnd,
                    WM_GETICON,
                    icon_type as usize,
                    0,
                    SMTO_ABORTIFHUNG,
                    40,
                    &mut result,
                )
            };
            if ok != 0 && result != 0 {
                return result as HICON;
            }
        }

        for index in [GCLP_HICONSM, GCLP_HICON] {
            let ptr = unsafe { GetClassLongPtrW(hwnd, index) };
            if ptr != 0 {
                return ptr as HICON;
            }
        }
        std::ptr::null_mut()
    }

    unsafe fn extract_exe_hicon_for_window(hwnd: HWND) -> Option<HICON> {
        let mut pid: u32 = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == 0 {
            return None;
        }
        let proc = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if proc.is_null() {
            return None;
        }

        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = unsafe { QueryFullProcessImageNameW(proc, 0, buf.as_mut_ptr(), &mut len) };
        unsafe { CloseHandle(proc) };
        if ok == 0 || len == 0 {
            return None;
        }

        buf.truncate(len as usize);
        // Ensure NUL-terminated UTF-16 path for ExtractIconExW.
        let mut wide: Vec<u16> = OsString::from_wide(&buf)
            .to_string_lossy()
            .encode_utf16()
            .collect();
        wide.push(0);

        let mut large: HICON = std::ptr::null_mut();
        let mut small: HICON = std::ptr::null_mut();
        let count = unsafe { ExtractIconExW(wide.as_ptr(), 0, &mut large, &mut small, 1) };
        if count == 0 {
            return None;
        }
        if !small.is_null() {
            if !large.is_null() {
                unsafe { DestroyIcon(large) };
            }
            Some(small)
        } else if !large.is_null() {
            Some(large)
        } else {
            None
        }
    }

    unsafe fn hicon_to_rgba(icon: HICON) -> Option<(u32, u32, Vec<u8>)> {
        let mut info = std::mem::MaybeUninit::<ICONINFO>::zeroed();
        if unsafe { GetIconInfo(icon, info.as_mut_ptr()) } == 0 {
            return None;
        }
        let info = unsafe { info.assume_init() };

        struct IconInfoGuard(ICONINFO);
        impl Drop for IconInfoGuard {
            fn drop(&mut self) {
                unsafe {
                    if !self.0.hbmColor.is_null() {
                        DeleteObject(self.0.hbmColor);
                    }
                    if !self.0.hbmMask.is_null() {
                        DeleteObject(self.0.hbmMask);
                    }
                }
            }
        }
        let _guard = IconInfoGuard(info);

        if info.hbmColor.is_null() {
            return None;
        }

        let mut bmp = std::mem::MaybeUninit::<BITMAP>::zeroed();
        let bytes_read = unsafe {
            GetObjectW(
                info.hbmColor,
                std::mem::size_of::<BITMAP>() as i32,
                bmp.as_mut_ptr().cast(),
            )
        };
        if bytes_read == 0 {
            return None;
        }
        let bmp = unsafe { bmp.assume_init() };
        let w = bmp.bmWidth.max(0) as u32;
        let h = bmp.bmHeight.max(0) as u32;
        if w == 0 || h == 0 || w > 512 || h > 512 {
            return None;
        }

        let hdc = unsafe { GetDC(std::ptr::null_mut()) };
        if hdc.is_null() {
            return None;
        }
        struct DcGuard(windows_sys::Win32::Graphics::Gdi::HDC);
        impl Drop for DcGuard {
            fn drop(&mut self) {
                unsafe { ReleaseDC(std::ptr::null_mut(), self.0) };
            }
        }
        let _dc_guard = DcGuard(hdc);

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32), // top-down scanlines
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: unsafe { std::mem::zeroed() },
        };

        let mut pixels = vec![0u8; (w * h * 4) as usize];
        let lines = unsafe {
            GetDIBits(
                hdc,
                info.hbmColor,
                0,
                h,
                pixels.as_mut_ptr().cast(),
                &mut bmi,
                DIB_RGB_COLORS,
            )
        };
        if lines == 0 {
            return None;
        }

        // Convert BGRA -> RGBA and check whether alpha channel has meaningful values.
        let mut any_alpha = false;
        let (px_chunks, _) = pixels.as_chunks_mut::<4>();
        for px in px_chunks {
            px.swap(0, 2);
            if px[3] != 0 {
                any_alpha = true;
            }
        }
        if !any_alpha {
            // Apply binary AND mask fallback if the source bitmap has zeroed alpha channel.
            let mut mask_pixels = vec![0u8; (w * h * 4) as usize];
            let mask_ok = if !info.hbmMask.is_null() {
                unsafe {
                    GetDIBits(
                        hdc,
                        info.hbmMask,
                        0,
                        h,
                        mask_pixels.as_mut_ptr().cast(),
                        &mut bmi,
                        DIB_RGB_COLORS,
                    ) != 0
                }
            } else {
                false
            };

            let (rgba_chunks, _) = pixels.as_chunks_mut::<4>();
            for (i, px) in rgba_chunks.iter_mut().enumerate() {
                let opaque = if mask_ok {
                    mask_pixels[i * 4] == 0
                } else {
                    true
                };
                px[3] = if opaque { 255 } else { 0 };
            }
        }

        if w > 32 || h > 32 {
            let img = image::RgbaImage::from_raw(w, h, pixels)?;
            let resized =
                image::imageops::resize(&img, 32, 32, image::imageops::FilterType::Triangle);
            Some((32, 32, resized.into_raw()))
        } else {
            Some((w, h, pixels))
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

    pub fn relaunch_as_admin() -> bool {
        false
    }

    pub fn is_window_process_elevated(_hwnd: isize) -> Option<bool> {
        None
    }

    pub fn get_client_size(_hwnd: isize) -> Option<(i32, i32)> {
        None
    }

    pub fn get_window_icon_rgba(_hwnd: isize) -> Option<(u32, u32, Vec<u8>)> {
        None
    }
}

pub use imp::{
    get_client_size, get_window_icon_rgba, is_current_process_elevated, is_window_process_elevated,
    relaunch_as_admin,
};
