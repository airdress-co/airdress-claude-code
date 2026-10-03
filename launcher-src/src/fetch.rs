//! Downloading, and what a download may not carry.
//!
//! Every request this launcher makes is a bare `GET` with a constant
//! `User-Agent`, no query string, no cookie, no `Referer` and no custom
//! header (FR-120). Nothing in it distinguishes one person's download
//! from another's, which is what makes our CDN's "we keep no request
//! record" worth saying: a log we do not keep and a request that
//! carries nothing are two different promises, and both are needed.
//!
//! Our CDN is tried first and GitHub second. Neither is trusted: a
//! failure is a connection error, a non-200, a timeout, or a hash that
//! does not match, and in every one of those cases the other origin is
//! tried before anything is refused.

use std::io::Read;
use std::time::Duration;

/// The only `User-Agent` this program sends. No version, no platform,
/// no hostname: a constant string tells a server that a launcher asked,
/// and nothing about which one.
pub const USER_AGENT: &str = "airdress-launch";

/// A bundle is large and a withdrawal list is small, so they wait
/// differently. Both are deliberately short: a stalled download must
/// fall through to the other origin rather than hang a session's start.
pub const BUNDLE_TIMEOUT: Duration = Duration::from_secs(60);
/// See [`BUNDLE_TIMEOUT`].
pub const DENYLIST_TIMEOUT: Duration = Duration::from_secs(5);

/// Most a single artifact may be. A bundle is a few megabytes; anything
/// an order of magnitude past that is not ours, and reading it into
/// memory first would be the attack.
pub const MAX_BYTES: usize = 64 * 1024 * 1024;

/// What one origin answered.
///
/// `Debug` deliberately does not print the bytes: a test that failed
/// would otherwise dump a whole bundle into the output.
pub struct Fetched {
    pub bytes: Vec<u8>,
    /// `cdn` or `github`.
    pub origin: &'static str,
}

/// Why a fetch from every origin failed, in the words of each.
#[derive(Debug)]
pub struct FetchFailed {
    pub attempts: Vec<(String, String)>,
}

impl std::fmt::Debug for Fetched {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Fetched {{ origin: {}, {} bytes }}",
            self.origin,
            self.bytes.len()
        )
    }
}

impl std::fmt::Display for FetchFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (origin, why) in &self.attempts {
            writeln!(f, "  {origin}: {why}")?;
        }
        Ok(())
    }
}

/// An agent with the hygiene rules built in.
fn agent(timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(timeout)
        .user_agent(USER_AGENT)
        // No cookie jar is configured, so none exists. `ureq` sends no
        // cookie unless one is set; this says so where somebody would
        // look for it.
        .redirects(5)
        .build()
}

/// Fetch one URL. No header beyond the agent's `User-Agent`.
pub fn get(url: &str, timeout: Duration) -> Result<Vec<u8>, String> {
    let resp = agent(timeout).get(url).call().map_err(|e| match e {
        ureq::Error::Status(code, _) => format!("HTTP {code}"),
        ureq::Error::Transport(t) => format!("{t}"),
    })?;
    let mut bytes = Vec::new();
    resp.into_reader()
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("read failed: {e}"))?;
    if bytes.len() > MAX_BYTES {
        return Err(format!("larger than {MAX_BYTES} bytes"));
    }
    Ok(bytes)
}

/// Try both origins, in order, with an optional hash to match.
///
/// A hash mismatch counts as that origin failing, which is the point:
/// one origin serving a wrong byte must not stop the install, and must
/// not be trusted either.
pub fn get_from_origins(
    origins: [(&'static str, &str); 2],
    timeout: Duration,
    expect_sha256: Option<&str>,
) -> Result<Fetched, FetchFailed> {
    let mut attempts = Vec::new();
    for (origin, url) in origins {
        match get(url, timeout) {
            Ok(bytes) => {
                if let Some(expected) = expect_sha256 {
                    let actual = sha256_hex(&bytes);
                    if actual != expected {
                        attempts.push((
                            origin.to_owned(),
                            format!("sha256 was {actual}, expected {expected}"),
                        ));
                        continue;
                    }
                }
                return Ok(Fetched { bytes, origin });
            }
            Err(why) => attempts.push((origin.to_owned(), why)),
        }
    }
    Err(FetchFailed { attempts })
}

/// Lowercase hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// A one-request server that records what it was asked, so the
    /// hygiene rules are measured rather than asserted about the code.
    fn capture(body: Vec<u8>, status: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);
        std::thread::spawn(move || {
            for stream in listener.incoming().take(4) {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut head = String::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        break;
                    }
                    if line.trim().is_empty() {
                        break;
                    }
                    head.push_str(&line);
                }
                recorder.lock().unwrap().push(head);
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.write_all(&body);
                let _ = stream.flush();
            }
        });
        (format!("http://127.0.0.1:{port}/artifact"), seen)
    }

    #[test]
    fn a_request_carries_nothing_identifying() {
        let (url, seen) = capture(b"payload".to_vec(), "200 OK");
        let bytes = get(&url, Duration::from_secs(5)).unwrap();
        assert_eq!(bytes, b"payload");

        let head = seen.lock().unwrap()[0].clone();
        let lower = head.to_lowercase();
        assert!(head.starts_with("GET /artifact HTTP/1.1"), "{head}");
        assert!(
            lower.contains("user-agent: airdress-launch\r\n"),
            "the User-Agent carries more than the constant: {head}"
        );
        for forbidden in ["cookie:", "referer:", "authorization:", "x-"] {
            assert!(
                !lower.contains(forbidden),
                "a request carried {forbidden}: {head}"
            );
        }
        assert!(
            !head.contains('?'),
            "a request carried a query string: {head}"
        );
        // One `User-Agent`, and no version or platform in it.
        assert_eq!(lower.matches("user-agent:").count(), 1, "{head}");
        for leak in ["linux", "darwin", "x86", "aarch64", "0.1.0"] {
            assert!(!lower.contains(leak), "the request names {leak}: {head}");
        }
    }

    #[test]
    fn the_second_origin_is_tried_when_the_first_fails() {
        let (bad, _) = capture(b"nope".to_vec(), "500 Server Error");
        let (good, _) = capture(b"payload".to_vec(), "200 OK");
        let expected = sha256_hex(b"payload");
        let fetched = get_from_origins(
            [("cdn", &bad), ("github", &good)],
            Duration::from_secs(5),
            Some(&expected),
        )
        .expect("the fallback origin");
        assert_eq!(fetched.origin, "github");
        assert_eq!(fetched.bytes, b"payload");
    }

    #[test]
    fn an_origin_serving_a_wrong_byte_is_a_failed_origin_not_a_refusal() {
        // The case the two origins exist for: one of them is wrong, the
        // install still happens, and the wrong one is not trusted.
        let (tampered, _) = capture(b"payload!".to_vec(), "200 OK");
        let (honest, _) = capture(b"payload".to_vec(), "200 OK");
        let expected = sha256_hex(b"payload");
        let fetched = get_from_origins(
            [("cdn", &tampered), ("github", &honest)],
            Duration::from_secs(5),
            Some(&expected),
        )
        .expect("the honest origin");
        assert_eq!(fetched.origin, "github");
    }

    #[test]
    fn both_origins_failing_says_what_each_said() {
        let (first, _) = capture(b"".to_vec(), "500 Server Error");
        let (second, _) = capture(b"".to_vec(), "404 Not Found");
        let failed = get_from_origins(
            [("cdn", &first), ("github", &second)],
            Duration::from_secs(5),
            None,
        )
        .expect_err("both failed");
        let text = failed.to_string();
        assert!(text.contains("cdn: HTTP 500"), "{text}");
        assert!(text.contains("github: HTTP 404"), "{text}");
    }

    #[test]
    fn the_hash_is_lowercase_hex() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
