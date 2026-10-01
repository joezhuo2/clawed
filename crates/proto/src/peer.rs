//! Same-user checks for both ends of the local socket.
//!
//! The hook refuses to talk to a server run by another user (a squatted pipe
//! name), and the app refuses clients run by another user. On Windows the
//! server pipe also gets a DACL that grants access to the current user only.

use interprocess::local_socket::PeerCreds;

/// True when the process on the other end of the socket runs as the current
/// user. Fails closed: unknown peers are rejected.
pub fn is_same_user(creds: &PeerCreds) -> bool {
    #[cfg(windows)]
    {
        creds.pid().is_some_and(imp::pid_is_current_user)
    }
    #[cfg(unix)]
    {
        // SAFETY: geteuid has no preconditions and cannot fail.
        creds.euid().is_some_and(|uid| uid == unsafe { libc::geteuid() })
    }
}

#[cfg(windows)]
pub use imp::{pid_is_current_user, pipe_sddl};

#[cfg(windows)]
mod imp {
    use std::io;

    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{
        GetLengthSid, GetTokenInformation, TokenUser, PSID, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    struct Handle(HANDLE);

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: we own the handle and close it once.
            unsafe { CloseHandle(self.0) };
        }
    }

    /// Calls `f` with the user SID of `process`'s token.
    fn with_user_sid<T>(process: HANDLE, f: impl FnOnce(PSID) -> io::Result<T>) -> io::Result<T> {
        // SAFETY: Win32 calls on handles we own; the TOKEN_USER buffer is
        // u64-aligned and sized by the first GetTokenInformation call.
        unsafe {
            let mut token = std::ptr::null_mut();
            if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
                return Err(io::Error::last_os_error());
            }
            let token = Handle(token);
            let mut len = 0u32;
            GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut len);
            if len == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut buf = vec![0u64; (len as usize).div_ceil(8)];
            if GetTokenInformation(token.0, TokenUser, buf.as_mut_ptr().cast(), len, &mut len) == 0 {
                return Err(io::Error::last_os_error());
            }
            let user = &*(buf.as_ptr() as *const TOKEN_USER);
            f(user.User.Sid)
        }
    }

    fn sid_bytes(sid: PSID) -> io::Result<Vec<u8>> {
        // SAFETY: `sid` points into a live TOKEN_USER buffer.
        unsafe {
            let n = GetLengthSid(sid) as usize;
            Ok(std::slice::from_raw_parts(sid as *const u8, n).to_vec())
        }
    }

    fn current_sid() -> io::Result<Vec<u8>> {
        // SAFETY: the pseudo handle needs no closing.
        with_user_sid(unsafe { GetCurrentProcess() }, sid_bytes)
    }

    /// True when process `pid` runs as the current user.
    pub fn pid_is_current_user(pid: u32) -> bool {
        let peer = || -> io::Result<Vec<u8>> {
            // SAFETY: plain OpenProcess; the handle is closed by `Handle`.
            let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
            if h.is_null() {
                return Err(io::Error::last_os_error());
            }
            let h = Handle(h);
            with_user_sid(h.0, sid_bytes)
        };
        match (peer(), current_sid()) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    }

    /// SDDL for the server pipe: protected DACL, full access for the current
    /// user only.
    pub fn pipe_sddl() -> io::Result<String> {
        // SAFETY: the pseudo handle needs no closing.
        let sid = with_user_sid(unsafe { GetCurrentProcess() }, |sid| unsafe {
            let mut s = std::ptr::null_mut();
            if ConvertSidToStringSidW(sid, &mut s) == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut n = 0;
            while *s.add(n) != 0 {
                n += 1;
            }
            let out = String::from_utf16_lossy(std::slice::from_raw_parts(s, n));
            LocalFree(s.cast());
            Ok(out)
        })?;
        Ok(format!("D:P(A;;GA;;;{sid})"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn own_process_is_current_user() {
            assert!(pid_is_current_user(std::process::id()));
        }

        #[test]
        fn system_process_is_not() {
            // PID 4 is the System process.
            assert!(!pid_is_current_user(4));
        }

        #[test]
        fn sddl_names_user_sid() {
            let s = pipe_sddl().unwrap();
            assert!(s.starts_with("D:P(A;;GA;;;S-1-5-"), "{s}");
        }
    }
}
