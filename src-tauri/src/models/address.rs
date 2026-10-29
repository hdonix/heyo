//! Where the "local model" setting really points. Selected text, questions
//! and dictation go to that address, so the settings page says when it is
//! not this computer, an extension is told whether what it asks stays here
//! (`AI.provider`), and what was copied is only ever given to a model at an
//! address that is (`clipboard/cases.rs`). Only loopback is: an address in
//! a private range (192.168.…, 10.…) is another machine, however near.

/// This computer, another (by name, and whether what is sent there is
/// encrypted), or nowhere a model could be.
#[derive(Debug, PartialEq)]
pub(crate) enum Address {
    ThisComputer,
    Elsewhere { host: String, encrypted: bool },
    /// Not an http(s) URL at all.
    Unknown,
}

pub(crate) fn address(url: &str) -> Address {
    use url::{Host, Url};
    let Ok(url) = Url::parse(url.trim()) else { return Address::Unknown };
    if url.scheme() != "http" && url.scheme() != "https" {
        return Address::Unknown;
    }
    let local = match url.host() {
        None => return Address::Unknown,
        Some(Host::Domain(name)) => {
            // "localhost." is the same name, fully qualified.
            let name = name.trim_end_matches('.');
            name == "localhost" || name.ends_with(".localhost")
        }
        // The URL parser writes every form of an IPv4 address (127.1,
        // 0x7f.0.0.1, 2130706433) as the address itself.
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        // IPv4 loopback written as IPv6 too.
        Some(Host::Ipv6(ip)) => ip.is_loopback() || ip.to_ipv4_mapped().is_some_and(|ip| ip.is_loopback()),
    };
    if local {
        return Address::ThisComputer;
    }
    let host = match url.port() {
        Some(port) => format!("{}:{port}", url.host_str().unwrap_or_default()),
        None => url.host_str().unwrap_or_default().to_string(),
    };
    Address::Elsewhere { host, encrypted: url.scheme() == "https" }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn elsewhere(host: &str, encrypted: bool) -> Address {
        Address::Elsewhere { host: host.into(), encrypted }
    }

    #[test]
    fn this_computer_is_known_by_every_name_loopback_has() {
        for local in [
            "http://127.0.0.1:8000/v1",
            "http://127.0.0.1",
            "http://127.8.9.10:11434/v1",
            "http://localhost:1234/v1",
            "http://LOCALHOST:1234/v1",
            "http://localhost.:1234/v1",
            "https://LOCALHOST./v1",
            "http://ollama.localhost:11434",
            "https://localhost/v1",
            "http://[::1]:8000/v1",
            "http://[0:0:0:0:0:0:0:1]:8000/v1",
            "http://[::ffff:127.0.0.1]:8000",
            // Other spellings of 127.0.0.1.
            "http://127.1:8000",
            "http://0x7f.0.0.1:8000",
            "http://2130706433:8000",
            "  http://127.0.0.1:8000/v1  ",
        ] {
            assert_eq!(address(local), Address::ThisComputer, "{local}");
        }
    }

    #[test]
    fn a_private_address_is_another_machine() {
        assert_eq!(address("http://192.168.1.20:8000/v1"), elsewhere("192.168.1.20:8000", false));
        assert_eq!(address("http://10.0.0.5:11434"), elsewhere("10.0.0.5:11434", false));
        assert_eq!(address("http://172.16.4.1/v1"), elsewhere("172.16.4.1", false));
        assert_eq!(address("http://[fd00::1]:8000"), elsewhere("[fd00::1]:8000", false));
        assert_eq!(address("http://gpu-box.local:8000/v1"), elsewhere("gpu-box.local:8000", false));
        assert_eq!(address("http://0.0.0.0:8000"), elsewhere("0.0.0.0:8000", false));
        assert_eq!(address("http://128.0.0.1:8000"), elsewhere("128.0.0.1:8000", false));
    }

    #[test]
    fn an_encrypted_connection_is_told_from_a_plain_one() {
        assert_eq!(address("https://api.example.com/v1"), elsewhere("api.example.com", true));
        assert_eq!(address(" https://models.example.com/v1 "), elsewhere("models.example.com", true));
        assert_eq!(address("HTTP://api.example.com/v1"), elsewhere("api.example.com", false));
    }

    #[test]
    fn a_loopback_name_that_is_only_part_of_the_address_does_not_fool_it() {
        // The host is what follows the "@".
        assert_eq!(address("http://127.0.0.1@evil.example/v1"), elsewhere("evil.example", false));
        assert_eq!(address("http://localhost:pw@evil.example/v1"), elsewhere("evil.example", false));
        assert_eq!(address("https://localhost.evil.example/v1"), elsewhere("localhost.evil.example", true));
        assert_eq!(address("https://127.0.0.1.evil.example/v1"), elsewhere("127.0.0.1.evil.example", true));
        assert_eq!(address("http://notlocalhost:8000"), elsewhere("notlocalhost:8000", false));
        assert_eq!(address("http://evil.example/?host=127.0.0.1"), elsewhere("evil.example", false));
        assert_eq!(address("http://evil.example#@localhost"), elsewhere("evil.example", false));
        assert_eq!(address("http://[::ffff:8.8.8.8]:8000"), elsewhere("[::ffff:808:808]:8000", false));
    }

    #[test]
    fn what_is_not_an_http_address_is_not_guessed_at() {
        for unknown in ["", "localhost:8000", "127.0.0.1", "file:///etc/passwd", "ftp://localhost/x", "http://", "not a url"] {
            assert_eq!(address(unknown), Address::Unknown, "{unknown}");
        }
    }
}
