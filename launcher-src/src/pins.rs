//! `pins.json` — what this plugin commit says the bundle is.
//!
//! The marketplace entry pins a commit of this repository; that commit
//! contains this file; this file names, per platform, the bundle's
//! SHA-256, the two URLs it may be fetched from, and the certificate
//! identity that must have signed it. So the trust chain is: the
//! marketplace pin decides the commit, and the commit decides
//! everything else. Neither download origin is trusted at any point.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The whole file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pins {
    /// The release these pins are for, e.g. `0.1.0`.
    pub version: String,
    /// Keyed by platform: `linux-x86_64`, `linux-aarch64`,
    /// `darwin-universal`.
    pub platforms: BTreeMap<String, Platform>,
    /// The identity that signs the withdrawal list — a different
    /// workflow from the one that signs a release, so a compromised
    /// release job cannot un-withdraw itself.
    pub denylist: Denylist,
}

/// One platform's bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Platform {
    /// Lowercase hex, 64 characters.
    pub sha256: String,
    /// The bundle, at both origins.
    pub bundle: Origins,
    /// Its Sigstore bundle (the signature, the certificate and the
    /// Rekor inclusion proof), at both origins.
    pub sigstore: Origins,
    /// SLSA provenance, where a release produced it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Origins>,
    /// The workflow and tag that must have signed it, exactly.
    pub cert_identity: String,
    /// The OIDC issuer that vouched for that identity, exactly.
    pub cert_issuer: String,
}

/// The withdrawal list's own pins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Denylist {
    pub list: Origins,
    pub sigstore: Origins,
    pub cert_identity: String,
    pub cert_issuer: String,
}

/// One artifact, at the two origins, in preference order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Origins {
    /// Ours. Preferred, because we can promise it keeps no record of
    /// who downloaded what.
    pub cdn: String,
    /// GitHub's. The fallback and the public record; GitHub keeps its
    /// own request logs, which the README says.
    pub github: String,
}

impl Origins {
    /// The two, in the order they are tried.
    pub fn in_order(&self) -> [(&'static str, &str); 2] {
        [("cdn", self.cdn.as_str()), ("github", self.github.as_str())]
    }
}

impl Pins {
    pub fn read(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let pins: Self =
            serde_json::from_str(text).map_err(|e| format!("pins.json is not valid: {e}"))?;
        pins.validate()?;
        Ok(pins)
    }

    /// Refuse a pins file that cannot do its job.
    ///
    /// Each of these would otherwise fail later, in a place where the
    /// message would be about a hash or a URL rather than about the
    /// pins file being wrong.
    fn validate(&self) -> Result<(), String> {
        if self.version.trim().is_empty() {
            return Err("pins.json names no version".into());
        }
        if self.platforms.is_empty() {
            return Err("pins.json names no platform".into());
        }
        for (name, p) in &self.platforms {
            if p.sha256.len() != 64 || !p.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(format!("{name}: sha256 is not 64 hex characters"));
            }
            if p.sha256.chars().any(|c| c.is_ascii_uppercase()) {
                return Err(format!("{name}: sha256 must be lowercase"));
            }
            for (origin, url) in p.bundle.in_order() {
                check_url(name, origin, url)?;
            }
            for (origin, url) in p.sigstore.in_order() {
                check_url(name, origin, url)?;
            }
            if p.cert_identity.trim().is_empty() || p.cert_issuer.trim().is_empty() {
                return Err(format!("{name}: the certificate identity is incomplete"));
            }
        }
        Ok(())
    }

    /// The platform this build runs on, as `pins.json` keys it.
    pub fn platform_key() -> &'static str {
        if cfg!(target_os = "macos") {
            "darwin-universal"
        } else if cfg!(target_arch = "aarch64") {
            "linux-aarch64"
        } else {
            "linux-x86_64"
        }
    }

    /// This platform's pin.
    pub fn for_this_platform(&self) -> Result<&Platform, String> {
        let key = Self::platform_key();
        self.platforms.get(key).ok_or_else(|| {
            format!(
                "this plugin version has no bundle for {key}; it pins {}",
                self.platforms
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
    }
}

/// A URL must be `https` and must be one of the two hosts we publish
/// to. A pins file that named a third host would be a redirect nobody
/// reviewed — the signature would still have to check out, but an
/// unexpected host is worth refusing before a request is made.
fn check_url(platform: &str, origin: &str, url: &str) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err(format!("{platform}/{origin}: {url} is not https"));
    }
    let host = url
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or_default();
    const ALLOWED: &[&str] = &[
        "downloads.airdress.co",
        "github.com",
        "objects.githubusercontent.com",
        "release-assets.githubusercontent.com",
    ];
    if !ALLOWED.contains(&host) {
        return Err(format!(
            "{platform}/{origin}: {host} is not one of this plugin's download origins"
        ));
    }
    if url.contains('?') || url.contains('#') {
        return Err(format!(
            "{platform}/{origin}: a pinned URL carries no query string"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> String {
        serde_json::json!({
            "version": "0.1.0",
            "platforms": {
                "linux-x86_64": {
                    "sha256": "a".repeat(64),
                    "bundle": {
                        "cdn": "https://downloads.airdress.co/claude-plugin/v0.1.0/airdress-linux-x86_64.mcpb",
                        "github": "https://github.com/airdress-co/claude-plugin/releases/download/v0.1.0/airdress-linux-x86_64.mcpb"
                    },
                    "sigstore": {
                        "cdn": "https://downloads.airdress.co/claude-plugin/v0.1.0/airdress-linux-x86_64.mcpb.sigstore.json",
                        "github": "https://github.com/airdress-co/claude-plugin/releases/download/v0.1.0/airdress-linux-x86_64.mcpb.sigstore.json"
                    },
                    "cert_identity": "https://github.com/airdress-co/claude-plugin/.github/workflows/release.yml@refs/tags/v0.1.0",
                    "cert_issuer": "https://token.actions.githubusercontent.com"
                }
            },
            "denylist": {
                "list": {
                    "cdn": "https://downloads.airdress.co/claude-plugin/denylist.json",
                    "github": "https://github.com/airdress-co/claude-plugin/raw/denylist/denylist.json"
                },
                "sigstore": {
                    "cdn": "https://downloads.airdress.co/claude-plugin/denylist.json.sigstore.json",
                    "github": "https://github.com/airdress-co/claude-plugin/raw/denylist/denylist.json.sigstore.json"
                },
                "cert_identity": "https://github.com/airdress-co/claude-plugin/.github/workflows/denylist.yml@refs/heads/main",
                "cert_issuer": "https://token.actions.githubusercontent.com"
            }
        })
        .to_string()
    }

    #[test]
    fn a_good_file_parses_and_both_origins_are_ordered() {
        let pins = Pins::parse(&good()).unwrap();
        assert_eq!(pins.version, "0.1.0");
        let p = &pins.platforms["linux-x86_64"];
        let [(first, cdn), (second, github)] = p.bundle.in_order();
        assert_eq!(first, "cdn");
        assert!(cdn.starts_with("https://downloads.airdress.co/"));
        assert_eq!(second, "github");
        assert!(github.starts_with("https://github.com/"));
    }

    #[test]
    fn a_hash_that_is_not_a_hash_is_refused() {
        let broken = good().replace(&"a".repeat(64), "deadbeef");
        let e = Pins::parse(&broken).unwrap_err();
        assert!(e.contains("64 hex"), "{e}");
    }

    #[test]
    fn an_uppercase_hash_is_refused_rather_than_folded() {
        // Two spellings of one hash is one spelling too many: the
        // comparison downstream is a string comparison.
        let broken = good().replace(&"a".repeat(64), &"A".repeat(64));
        assert!(Pins::parse(&broken).unwrap_err().contains("lowercase"));
    }

    #[test]
    fn a_url_outside_the_two_origins_is_refused_before_any_request() {
        let broken = good().replace("https://downloads.airdress.co", "https://example.test");
        let e = Pins::parse(&broken).unwrap_err();
        assert!(
            e.contains("not one of this plugin's download origins"),
            "{e}"
        );
    }

    #[test]
    fn plain_http_is_refused() {
        let broken = good().replace("https://github.com", "http://github.com");
        assert!(Pins::parse(&broken).unwrap_err().contains("not https"));
    }

    #[test]
    fn a_query_string_in_a_pin_is_refused() {
        // Nothing identifying leaves this machine, and a pinned URL
        // carrying `?from=` would be exactly that, signed off by us.
        let broken = good().replace(
            "airdress-linux-x86_64.mcpb\"",
            "airdress-linux-x86_64.mcpb?from=plugin\"",
        );
        assert!(Pins::parse(&broken)
            .unwrap_err()
            .contains("no query string"));
    }

    /// The committed example must parse with this parser, and must
    /// name every platform the plugin claims to support. It is the only
    /// pins file in the tree between releases, so it is the only one
    /// that can go stale unnoticed.
    #[test]
    fn the_committed_example_parses_and_covers_every_platform() {
        const EXAMPLE: &str = include_str!("../../plugins/airdress/launcher/pins.example.json");
        let pins = Pins::parse(EXAMPLE).expect("the committed example");
        for platform in ["linux-x86_64", "linux-aarch64", "darwin-universal"] {
            assert!(
                pins.platforms.contains_key(platform),
                "the example pins no {platform}"
            );
        }
        // And this build's own platform is one of them, so a developer
        // on any supported machine can read their own case.
        assert!(pins.for_this_platform().is_ok());
    }

    #[test]
    fn an_unpinned_platform_says_what_is_pinned() {
        let pins = Pins::parse(&good()).unwrap();
        // The file above pins only x86_64 Linux, so on any other
        // platform the error names what there is.
        if Pins::platform_key() != "linux-x86_64" {
            let e = pins.for_this_platform().unwrap_err();
            assert!(e.contains("linux-x86_64"), "{e}");
        } else {
            assert!(pins.for_this_platform().is_ok());
        }
    }
}
