//! Resolves the per-user local socket name.

use std::io;

use interprocess::local_socket::{prelude::*, Name};

/// Named pipe `\\.\pipe\islet-<user>` on Windows.
#[cfg(windows)]
pub fn name() -> io::Result<Name<'static>> {
    use interprocess::local_socket::GenericNamespaced;
    crate::socket_name().to_ns_name::<GenericNamespaced>()
}

/// Unix socket in the per-user temp dir (`$TMPDIR` is 0700 on macOS).
#[cfg(not(windows))]
pub fn name() -> io::Result<Name<'static>> {
    use interprocess::local_socket::GenericFilePath;
    socket_path().to_fs_name::<GenericFilePath>()
}

#[cfg(not(windows))]
pub fn socket_path() -> std::path::PathBuf {
    let name = crate::socket_name();
    if name.contains('/') {
        return name.into();
    }
    std::env::temp_dir().join(format!("{name}.sock"))
}
