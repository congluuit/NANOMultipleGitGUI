//! Client side used by the `nanomgg-credential` sidecar.

use std::io::{self, BufRead, BufReader, Write};

use interprocess::local_socket::{Stream, prelude::*};
use serde::Serialize;
use zeroize::Zeroizing;

use crate::protocol::{AskpassRequest, CredentialRequest, CredentialResponse};
use crate::socket::SocketAddress;

pub fn request(
    address: &SocketAddress,
    request: &CredentialRequest,
) -> io::Result<CredentialResponse> {
    send(address, request)
}

/// Forwards an `SSH_ASKPASS` prompt; a passphrase comes back as [`CredentialResponse::Secret`].
pub fn askpass(
    address: &SocketAddress,
    request: &AskpassRequest,
) -> io::Result<CredentialResponse> {
    send(address, request)
}

fn send(address: &SocketAddress, request: &impl Serialize) -> io::Result<CredentialResponse> {
    let conn = Stream::connect(address.to_name()?)?;
    let mut reader = BufReader::new(conn);
    let mut line = Zeroizing::new(serde_json::to_string(request)?);
    line.push('\n');
    reader.get_mut().write_all(line.as_bytes())?;
    reader.get_mut().flush()?;

    let mut response = Zeroizing::new(String::new());
    reader.read_line(&mut response)?;
    if response.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "credential server closed the connection",
        ));
    }
    serde_json::from_str(&response).map_err(io::Error::other)
}
