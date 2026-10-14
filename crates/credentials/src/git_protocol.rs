//! The `git credential` helper protocol: `key=value` lines terminated by a blank line or EOF.
//! See `git help credential` ("INPUT/OUTPUT FORMAT").

use std::collections::BTreeMap;
use std::io::BufRead;

/// Reads attributes until a blank line or EOF. Array keys (`key[]`) keep the last value,
/// which is all a credential helper needs.
pub fn read_attributes(input: impl BufRead) -> std::io::Result<BTreeMap<String, String>> {
    let mut attrs = BTreeMap::new();
    for line in input.lines() {
        let line = line?;
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once('=') {
            attrs.insert(key.trim_end_matches("[]").to_owned(), value.to_owned());
        }
    }
    Ok(attrs)
}

/// Formats the answer to a `get` request.
pub fn format_credential(username: &str, password: &str) -> String {
    format!("username={username}\npassword={password}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_until_blank_line() {
        let input = "protocol=https\nhost=github.com\npath=acme/api.git\n\nignored=1\n";
        let attrs = read_attributes(input.as_bytes()).unwrap();
        assert_eq!(attrs.len(), 3);
        assert_eq!(attrs["host"], "github.com");
        assert_eq!(attrs["path"], "acme/api.git");
    }

    #[test]
    fn handles_crlf_eof_and_values_with_equals() {
        let input = "protocol=https\r\nhost=127.0.0.1:8080\r\nwwwauth[]=Basic realm=\"x\"";
        let attrs = read_attributes(input.as_bytes()).unwrap();
        assert_eq!(attrs["host"], "127.0.0.1:8080");
        assert_eq!(attrs["wwwauth"], "Basic realm=\"x\"");
    }

    #[test]
    fn formats_answer() {
        assert_eq!(
            format_credential("bob", "s3"),
            "username=bob\npassword=s3\n"
        );
    }
}
