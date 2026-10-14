//! Platform-specific local socket naming: a named pipe on Windows, a Unix-domain socket
//! inside a private (0700) directory elsewhere.

use std::io;

use interprocess::local_socket::Name;
#[cfg(unix)]
use interprocess::local_socket::{GenericFilePath, ToFsName};
#[cfg(windows)]
use interprocess::local_socket::{GenericNamespaced, ToNsName};

/// The value placed in `NANOMGG_SOCKET`: a pipe name on Windows, a socket path on Unix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketAddress(pub String);

impl SocketAddress {
    pub fn to_name(&self) -> io::Result<Name<'_>> {
        #[cfg(windows)]
        {
            self.0.as_str().to_ns_name::<GenericNamespaced>()
        }
        #[cfg(unix)]
        {
            self.0.as_str().to_fs_name::<GenericFilePath>()
        }
    }

    /// A fresh, unguessable address for this process.
    pub(crate) fn generate(id: &str) -> io::Result<Self> {
        #[cfg(windows)]
        {
            Ok(Self(format!(
                "nanomgg-credential-{}-{id}",
                std::process::id()
            )))
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
            let dir = std::env::temp_dir().join(format!("nanomgg-{id}"));
            std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
            Ok(Self(dir.join("cred.sock").to_string_lossy().into_owned()))
        }
    }

    pub(crate) fn cleanup(&self) {
        #[cfg(unix)]
        if let Some(dir) = std::path::Path::new(&self.0).parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
