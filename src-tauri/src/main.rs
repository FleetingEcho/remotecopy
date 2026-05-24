#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Single-instance guard: open or create a named mutex before Tauri starts.
    // Uses OpenMutexW + CreateMutexW — no GetLastError race, no dependency on
    // tauri-plugin-single-instance.  A second process exits immediately without
    // ever creating a window or tray icon.
    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use std::ptr;

        extern "system" {
            // kernel32
            fn OpenMutexW(access: u32, inherit: i32, name: *const u16) -> *mut std::ffi::c_void;
            fn CreateMutexW(attrs: *const std::ffi::c_void, owner: i32, name: *const u16) -> *mut std::ffi::c_void;
            fn CloseHandle(h: *mut std::ffi::c_void) -> i32;
            // user32
            fn FindWindowW(class: *const u16, title: *const u16) -> *mut std::ffi::c_void;
            fn ShowWindow(hwnd: *mut std::ffi::c_void, cmd: i32) -> i32;
            fn SetForegroundWindow(hwnd: *mut std::ffi::c_void) -> i32;
        }

        const SYNCHRONIZE: u32 = 0x00100000;
        const SW_SHOW: i32 = 5;

        let name: Vec<u16> = OsStr::new("RemoteCopy-GUI-Instance-v2")
            .encode_wide()
            .chain(Some(0))
            .collect();

        unsafe {
            // Step 1: try to open an existing mutex
            let existing = OpenMutexW(SYNCHRONIZE, 0, name.as_ptr());
            if !existing.is_null() {
                CloseHandle(existing);

                // Find the existing window by title and bring it to front
                let title: Vec<u16> = OsStr::new("RemoteCopy")
                    .encode_wide()
                    .chain(Some(0))
                    .collect();
                let hwnd = FindWindowW(ptr::null(), title.as_ptr());
                if !hwnd.is_null() {
                    ShowWindow(hwnd, SW_SHOW);
                    SetForegroundWindow(hwnd);
                }

                std::process::exit(0);
            }

            // Step 2: mutex doesn't exist yet — create it (we're the first instance)
            let _h = CreateMutexW(ptr::null(), 0, name.as_ptr());
            // Keep handle alive for the process lifetime (leaked intentionally —
            // Windows auto-closes it on process exit).
        }
    }

    remotecopy_gui_lib::run();
}
