use std::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalHotkeyAction {
    ToggleTriggers,
    EmergencyStop,
    ToggleWindow,
}

impl GlobalHotkeyAction {
    pub fn key_label(self) -> &'static str {
        match self {
            Self::ToggleTriggers => "F8",
            Self::EmergencyStop => "F9",
            Self::ToggleWindow => "F10",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ToggleTriggers => "开关实时辅助",
            Self::EmergencyStop => "紧急急停",
            Self::ToggleWindow => "显隐窗口",
        }
    }
}

/// Global hotkey listener backed by standard Win32 `RegisterHotKey` (no low-level hooks).
pub struct GlobalHotkeyListener {
    rx: mpsc::Receiver<GlobalHotkeyAction>,
    failed_keys: Vec<&'static str>,
    #[cfg(windows)]
    thread_id: u32,
    #[cfg(windows)]
    join_handle: Option<std::thread::JoinHandle<()>>,
}

impl GlobalHotkeyListener {
    pub fn start<F>(on_action: F) -> Self
    where
        F: Fn(GlobalHotkeyAction) + Send + Sync + 'static,
    {
        imp::start(on_action)
    }

    pub fn try_recv(&self) -> Option<GlobalHotkeyAction> {
        self.rx.try_recv().ok()
    }

    pub fn failed_keys(&self) -> &[&'static str] {
        &self.failed_keys
    }
}

#[cfg(windows)]
mod imp {
    use std::sync::mpsc;

    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey, VK_F8, VK_F9, VK_F10,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetMessageW, MSG, PostThreadMessageW, WM_HOTKEY, WM_QUIT,
    };

    use super::{GlobalHotkeyAction, GlobalHotkeyListener};

    const HOTKEYS: &[(i32, u32, GlobalHotkeyAction)] = &[
        (1, VK_F8 as u32, GlobalHotkeyAction::ToggleTriggers),
        (2, VK_F9 as u32, GlobalHotkeyAction::EmergencyStop),
        (3, VK_F10 as u32, GlobalHotkeyAction::ToggleWindow),
    ];

    pub fn start<F>(on_action: F) -> GlobalHotkeyListener
    where
        F: Fn(GlobalHotkeyAction) + Send + Sync + 'static,
    {
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel::<(u32, Vec<&'static str>)>(1);

        let join_handle = std::thread::Builder::new()
            .name("moca-global-hotkeys".into())
            .spawn(move || {
                let tid = unsafe { GetCurrentThreadId() };
                let mut registered_ids = Vec::with_capacity(HOTKEYS.len());
                let mut failed_keys = Vec::new();

                for &(id, vk, action) in HOTKEYS {
                    let ok = unsafe { RegisterHotKey(std::ptr::null_mut(), id, MOD_NOREPEAT, vk) };
                    if ok != 0 {
                        registered_ids.push(id);
                    } else {
                        failed_keys.push(action.key_label());
                    }
                }

                // Ensure the thread message queue exists before notifying the parent thread.
                let mut msg: MSG = unsafe { std::mem::zeroed() };
                let _ = ready_tx.send((tid, failed_keys));

                loop {
                    let ret = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
                    if ret <= 0 {
                        break;
                    }
                    if msg.message == WM_HOTKEY {
                        let matched = HOTKEYS
                            .iter()
                            .find(|&&(id, _, _)| id == msg.wParam as i32)
                            .map(|&(_, _, action)| action);
                        if let Some(action) = matched {
                            on_action(action);
                            let _ = tx.send(action);
                        }
                    }
                }

                for id in registered_ids {
                    unsafe {
                        UnregisterHotKey(std::ptr::null_mut(), id);
                    }
                }
            })
            .ok();

        let (thread_id, failed_keys) = match join_handle.as_ref() {
            Some(_) => ready_rx.recv().unwrap_or((0, Vec::new())),
            None => (0, vec!["F8", "F9", "F10"]),
        };

        GlobalHotkeyListener {
            rx,
            failed_keys,
            thread_id,
            join_handle,
        }
    }

    impl Drop for GlobalHotkeyListener {
        fn drop(&mut self) {
            if self.thread_id != 0 {
                unsafe {
                    PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
                }
            }
            if let Some(handle) = self.join_handle.take() {
                let _ = handle.join();
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::sync::mpsc;

    use super::{GlobalHotkeyAction, GlobalHotkeyListener};

    pub fn start<F>(_on_action: F) -> GlobalHotkeyListener
    where
        F: Fn(GlobalHotkeyAction) + Send + Sync + 'static,
    {
        let (_tx, rx) = mpsc::channel();
        GlobalHotkeyListener {
            rx,
            failed_keys: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GlobalHotkeyAction, GlobalHotkeyListener};

    #[test]
    fn hotkey_labels_match_expected() {
        assert_eq!(GlobalHotkeyAction::ToggleTriggers.key_label(), "F8");
        assert_eq!(GlobalHotkeyAction::ToggleTriggers.label(), "开关实时辅助");
        assert_eq!(GlobalHotkeyAction::EmergencyStop.key_label(), "F9");
        assert_eq!(GlobalHotkeyAction::EmergencyStop.label(), "紧急急停");
        assert_eq!(GlobalHotkeyAction::ToggleWindow.key_label(), "F10");
        assert_eq!(GlobalHotkeyAction::ToggleWindow.label(), "显隐窗口");
    }

    #[test]
    fn hotkey_listener_starts_and_drops_cleanly() {
        let listener = GlobalHotkeyListener::start(|_| {});
        assert!(listener.try_recv().is_none());
        drop(listener);
    }
}
