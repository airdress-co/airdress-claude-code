//! Offline Sigstore verification against an embedded trust root.
//!
//! Four things have to be true before anything executes:
//!
//! 1. the bytes hash to the pinned SHA-256;
//! 2. the signing certificate chains to Fulcio, and was valid when it
//!    signed;
//! 3. its identity and issuer are **exactly** what this plugin commit
//!    pins — the release workflow, at that tag;
//! 4. the signature covers the artifact's digest, and Rekor's inclusion
//!    proof checks out against Rekor's own key.
//!
//! All of it offline (`offline = true`): the proof travels in the
//! Sigstore bundle, so nothing has to be asked of a log server at start
//! time, and a verification that needed the network would be a
//! verification that could be denied.
//!
//! The trust root is **embedded in this binary** and never refreshed
//! online (D-30). Each plugin release ships the current one. A launcher
//! too old to verify a current signature fails closed and says to
//! update the plugin — which is the honest failure, because the
//! alternative is a program that fetches its own idea of who to trust.

use sigstore::bundle::verify::{blocking::Verifier, policy};
use sigstore::bundle::Bundle;
use sigstore::rekor::apis::configuration::Configuration as RekorConfiguration;
use sigstore::trust::ManualTrustRoot;

/// Sigstore's public-good trust root, as of this plugin release.
///
/// Refreshed by a maintainer, per release, and reviewed like any other
/// change: `scripts/refresh-trust-root.sh` fetches it and the diff is
/// what a reviewer reads.
const TRUSTED_ROOT: &str = include_str!("../../plugins/airdress/launcher/trust/trusted_root.json");

/// What went wrong, in the words the launcher prints.
#[derive(Debug)]
pub struct VerifyError {
    /// The check that failed, named: `sha256`, `certificate identity`,
    /// `signature`, `rekor inclusion`, `trust root`.
    pub check: &'static str,
    pub expected: String,
    pub actual: String,
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} did not match\n  expected: {}\n  actual:   {}",
            self.check, self.expected, self.actual
        )
    }
}

/// The embedded trust root, parsed.
///
/// `trusted_root.json` is Sigstore's own published format. Only the
/// three pieces a verifier needs are taken out of it: Fulcio's
/// certificates, Rekor's log keys and the certificate-transparency log
/// keys.
fn trust_root() -> Result<ManualTrustRoot<'static>, VerifyError> {
    use base64::Engine as _;
    use std::collections::BTreeMap;

    let fail = |what: String| VerifyError {
        check: "trust root",
        expected: "Sigstore's published trusted_root.json".into(),
        actual: what,
    };

    let doc: serde_json::Value =
        serde_json::from_str(TRUSTED_ROOT).map_err(|e| fail(format!("not JSON: {e}")))?;
    let b64 = base64::engine::general_purpose::STANDARD;

    let mut fulcio_certs = Vec::new();
    for ca in doc["certificateAuthorities"]
        .as_array()
        .into_iter()
        .flatten()
    {
        for cert in ca["certChain"]["certificates"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let raw = cert["rawBytes"]
                .as_str()
                .ok_or_else(|| fail("a certificate has no rawBytes".into()))?;
            let der = b64
                .decode(raw)
                .map_err(|e| fail(format!("a certificate is not base64: {e}")))?;
            fulcio_certs.push(der.into());
        }
    }
    if fulcio_certs.is_empty() {
        return Err(fail("no Fulcio certificates".into()));
    }

    let keys = |field: &str| -> Result<BTreeMap<String, Vec<u8>>, VerifyError> {
        let mut out = BTreeMap::new();
        for log in doc[field].as_array().into_iter().flatten() {
            let id = log["logId"]["keyId"]
                .as_str()
                .ok_or_else(|| fail(format!("a {field} entry has no logId")))?;
            let raw = log["publicKey"]["rawBytes"]
                .as_str()
                .ok_or_else(|| fail(format!("a {field} key has no rawBytes")))?;
            let id = b64
                .decode(id)
                .map_err(|e| fail(format!("a {field} logId is not base64: {e}")))?;
            let key = b64
                .decode(raw)
                .map_err(|e| fail(format!("a {field} key is not base64: {e}")))?;
            out.insert(hex::encode(id), key);
        }
        Ok(out)
    };

    let rekor_keys = keys("tlogs")?;
    if rekor_keys.is_empty() {
        return Err(fail("no Rekor keys".into()));
    }
    Ok(ManualTrustRoot {
        fulcio_certs,
        rekor_keys,
        ctfe_keys: keys("ctlogs")?,
    })
}

/// The Rekor client configuration the verifier is built with.
///
/// Verification is offline, so this client never makes a request; it
/// exists only because `Verifier::new` takes one. `Default::default()`
/// would build it with `reqwest::Client::new()`, which loads the
/// system's CA certificates and **panics** on a machine that has none
/// (a minimal container, measured on Debian slim 2026-10-09). An
/// offline check must not depend on the system trust store, so the
/// client trusts no roots at all: if anything ever did use it, the TLS
/// handshake would fail closed rather than trust a store we did not
/// choose.
fn offline_rekor_config() -> Result<RekorConfiguration, VerifyError> {
    let client = reqwest::Client::builder()
        .tls_certs_only(std::iter::empty())
        .build()
        .map_err(|e| VerifyError {
            check: "trust root",
            expected: "an offline verifier".into(),
            actual: format!("could not set one up: {e}"),
        })?;
    Ok(RekorConfiguration {
        base_path: String::new(),
        user_agent: None,
        client,
        basic_auth: None,
        oauth_access_token: None,
        bearer_access_token: None,
        api_key: None,
    })
}

/// Verify one artifact.
///
/// `bundle_json` is the Sigstore bundle as published beside it.
pub fn artifact(
    bytes: &[u8],
    expected_sha256: &str,
    bundle_json: &[u8],
    cert_identity: &str,
    cert_issuer: &str,
) -> Result<(), VerifyError> {
    // 1. The hash, first and cheapest. A mismatch here says which byte
    //    count we got, which is what a person debugging wants.
    let actual = crate::fetch::sha256_hex(bytes);
    if actual != expected_sha256 {
        return Err(VerifyError {
            check: "sha256",
            expected: expected_sha256.to_owned(),
            actual,
        });
    }

    let bundle: Bundle = serde_json::from_slice(bundle_json).map_err(|e| VerifyError {
        check: "signature",
        expected: "a Sigstore bundle".into(),
        actual: format!("unreadable: {e}"),
    })?;

    let verifier =
        Verifier::new(offline_rekor_config()?, trust_root()?).map_err(|e| VerifyError {
            check: "trust root",
            expected: "a usable trust root".into(),
            actual: format!("{e}"),
        })?;

    // 2–4. Chain, identity, signature, Rekor inclusion. `offline` is
    //      true: the proof is in the bundle.
    let policy = policy::Identity::new(cert_identity, cert_issuer);
    let mut digest = <sha2::Sha256 as sha2::Digest>::new();
    sha2::Digest::update(&mut digest, bytes);
    verifier
        .verify_digest(digest, bundle, &policy, true)
        .map_err(|e| VerifyError {
            check: check_name(&format!("{e}")),
            expected: format!("{cert_identity} signed by {cert_issuer}"),
            actual: format!("{e}"),
        })
}

/// Name the check a verification error belongs to.
///
/// The library's error strings are precise and long. A person reading a
/// refusal wants to know which of the four checks failed first — "the
/// identity is wrong" and "the proof is missing" lead to different next
/// steps.
fn check_name(message: &str) -> &'static str {
    let m = message.to_ascii_lowercase();
    if m.contains("policy") || m.contains("identity") || m.contains("san") {
        "certificate identity"
    } else if m.contains("rekor") || m.contains("inclusion") || m.contains("checkpoint") {
        "rekor inclusion"
    } else if m.contains("certificate") || m.contains("chain") || m.contains("expired") {
        "certificate chain"
    } else {
        "signature"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The verifier needs no system trust store. With
    /// `Default::default()` this panicked inside reqwest when run in a
    /// container with no CA certificates (debian:stable-slim); the
    /// workstation has them, so here it only proves the client builds.
    #[test]
    fn the_verifier_is_built_without_system_roots() {
        Verifier::new(
            offline_rekor_config().expect("a client with no roots"),
            trust_root().unwrap(),
        )
        .expect("a verifier");
    }

    #[test]
    fn the_embedded_trust_root_parses_and_carries_what_a_verifier_needs() {
        // The one thing that would make every verification fail at
        // once, and that a refresh could break silently.
        let root = trust_root().expect("the embedded trust root");
        assert!(
            !root.fulcio_certs.is_empty(),
            "no Fulcio certificates: nothing could chain"
        );
        assert!(
            !root.rekor_keys.is_empty(),
            "no Rekor keys: no inclusion proof could be checked"
        );
        assert!(
            !root.ctfe_keys.is_empty(),
            "no certificate-transparency keys"
        );
    }

    #[test]
    fn a_verifier_can_be_built_from_it() {
        trust_root()
            .and_then(|r| {
                Verifier::new(offline_rekor_config()?, r).map_err(|e| VerifyError {
                    check: "trust root",
                    expected: "a usable trust root".into(),
                    actual: format!("{e}"),
                })
            })
            .expect("a verifier");
    }

    #[test]
    fn a_wrong_hash_fails_before_anything_else_is_read() {
        // The bundle here is nonsense. The hash check must refuse
        // first, so a tampered artifact never reaches a parser.
        let e = artifact(
            b"the wrong bytes",
            &"0".repeat(64),
            b"not a bundle at all",
            "whoever",
            "wherever",
        )
        .unwrap_err();
        assert_eq!(e.check, "sha256");
        assert_eq!(e.expected, "0".repeat(64));
    }

    #[test]
    fn an_unreadable_bundle_is_a_signature_failure_naming_itself() {
        let bytes = b"payload";
        let e = artifact(
            bytes,
            &crate::fetch::sha256_hex(bytes),
            b"{ not a bundle }",
            "whoever",
            "wherever",
        )
        .unwrap_err();
        assert_eq!(e.check, "signature");
        assert!(e.actual.contains("unreadable"), "{}", e.actual);
    }

    #[test]
    fn errors_are_sorted_into_the_four_checks() {
        assert_eq!(check_name("policy mismatch: SAN"), "certificate identity");
        assert_eq!(
            check_name("rekor inclusion proof invalid"),
            "rekor inclusion"
        );
        assert_eq!(check_name("certificate expired"), "certificate chain");
        assert_eq!(check_name("signature does not verify"), "signature");
    }

    #[test]
    fn a_refusal_prints_the_check_and_both_values() {
        let shown = VerifyError {
            check: "sha256",
            expected: "aaa".into(),
            actual: "bbb".into(),
        }
        .to_string();
        assert!(shown.contains("sha256 did not match"));
        assert!(shown.contains("expected: aaa"));
        assert!(shown.contains("actual:   bbb"));
    }
}
