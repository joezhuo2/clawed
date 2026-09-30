//! Parent process id: the `claude` process that ran this hook.

#[cfg(unix)]
pub fn parent_pid() -> u32 {
    std::os::unix::process::parent_id()
}

#[cfg(windows)]
pub fn parent_pid() -> u32 {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;

    // SAFETY: plain Win32 calls on a snapshot handle we own and close.
    unsafe {
        let me = GetCurrentProcessId();
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return 0;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ppid = 0;
        let mut ok = Process32FirstW(snap, &mut entry);
        while ok != 0 {
            if entry.th32ProcessID == me {
                ppid = entry.th32ParentProcessID;
                break;
            }
            ok = Process32NextW(snap, &mut entry);
        }
        CloseHandle(snap);
        ppid
    }
}
