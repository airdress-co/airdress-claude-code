//! The withdrawal list, and why it fails closed.
//!
//! A release that turns out to be bad has to be stoppable after the
//! fact. A signed list of withdrawn versions does that, and the
//! interesting decision is what happens when the list cannot be
//! fetched: this **refuses** (D-35, FR-75).
//!
//! The reason is the shape of the failure. A launcher that runs
//! whatever it has when it cannot check is a launcher an attacker makes
//! unable to check — blocking one hostname is easier than forging a
//! signature. So a cached list is good for fourteen days from its
//! `issued_at`, and past that the answer is "I cannot confirm this was
//! not withdrawn", with one named escape hatch that prints itself on
//! every start.
//!
//! Fourteen days is the compromise: it is longer than a holiday
//! offline, and shorter than a release cycle, so somebody genuinely
//! disconnected keeps working while a withdrawal still lands.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// How long a cached list is trusted, from its own `issued_at`.
pub const VALIDITY: Duration = Duration::from_secs(14 * 24 * 60 * 60);

/// Set to an exact version to run one that was withdrawn.
pub const ENV_ALLOW_YANKED: &str = "AIRDRESS_MCP_ALLOW_YANKED";
/// Set to an exact version to run without a current withdrawal list.
pub const ENV_OFFLINE_OK: &str = "AIRDRESS_MCP_OFFLINE_OK";

/// The list, as published.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Denylist {
    /// RFC 3339. Re-signed weekly even when nothing changed, so that
    /// "the list is old" and "nothing was withdrawn" cannot be confused.
    pub issued_at: String,
    #[serde(default)]
    pub yanked: Vec<Yanked>,
}

/// One withdrawn version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Yanked {
    pub version: String,
    pub reason: String,
    /// RFC 3339, the day it was withdrawn.
    pub since: String,
}

/// What the list says about the version about to run.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Not withdrawn, by a list new enough to believe.
    Clear,
    /// Withdrawn.
    Withdrawn { reason: String, since: String },
    /// No list, or none issued within the validity window.
    Unconfirmed { last_issued: Option<String> },
}

impl Denylist {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let list: Self = serde_json::from_slice(bytes)
            .map_err(|e| format!("the withdrawal list is not readable: {e}"))?;
        if parse_rfc3339(&list.issued_at).is_none() {
            return Err(format!(
                "the withdrawal list's issued_at is not a timestamp: {}",
                list.issued_at
            ));
        }
        Ok(list)
    }

    /// What this list says about `version`, as of `now`.
    pub fn verdict(&self, version: &str, now: SystemTime) -> Verdict {
        let issued = parse_rfc3339(&self.issued_at);
        let fresh = match issued {
            Some(at) => now
                .duration_since(at)
                .map(|age| age <= VALIDITY)
                // A list issued in the future is not stale; a clock is
                // more often wrong than a signature.
                .unwrap_or(true),
            None => false,
        };
        if !fresh {
            return Verdict::Unconfirmed {
                last_issued: Some(self.issued_at.clone()),
            };
        }
        match self.yanked.iter().find(|y| y.version == version) {
            Some(y) => Verdict::Withdrawn {
                reason: y.reason.clone(),
                since: y.since.clone(),
            },
            None => Verdict::Clear,
        }
    }
}

/// The sentence a verdict produces, or `Ok` to carry on.
///
/// Both overrides name an exact version: a blanket `=1` would be set
/// once and forgotten, and would then cover a version nobody considered.
pub fn decide(verdict: &Verdict, version: &str, env: impl Fn(&str) -> Option<String>) -> Decision {
    match verdict {
        Verdict::Clear => Decision::Run {
            override_note: None,
        },
        Verdict::Withdrawn { reason, since } => {
            if env(ENV_ALLOW_YANKED).as_deref() == Some(version) {
                Decision::Run {
                    override_note: Some(format!(
                        "{ENV_ALLOW_YANKED}={version}: running a version withdrawn on {since} \
                         ({reason})"
                    )),
                }
            } else {
                Decision::Refuse(format!(
                    "v{version} was withdrawn on {since}: {reason}. Update the plugin."
                ))
            }
        }
        Verdict::Unconfirmed { last_issued } => {
            if env(ENV_OFFLINE_OK).as_deref() == Some(version) {
                Decision::Run {
                    override_note: Some(format!(
                        "{ENV_OFFLINE_OK}={version}: running without a current withdrawal list"
                    )),
                }
            } else {
                let last = last_issued
                    .as_deref()
                    .map_or_else(|| "never".to_owned(), |d| format!("from {d}"));
                Decision::Refuse(format!(
                    "cannot confirm v{version} has not been withdrawn: the last withdrawal \
                     list is {last}. Connect to the internet once, or run with \
                     {ENV_OFFLINE_OK}={version} to accept the risk."
                ))
            }
        }
    }
}

/// What to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// Carry on. `override_note` is printed on **every** start, not
    /// once: a weakened check must never become quiet.
    Run {
        override_note: Option<String>,
    },
    Refuse(String),
}

/// Where a fetched list is kept between starts.
pub fn cache_path(state_dir: &Path) -> PathBuf {
    state_dir.join("denylist.json")
}

/// RFC 3339, to the extent this needs it: `YYYY-MM-DDTHH:MM:SSZ` and
/// the common offsets. Written out rather than taken from a date
/// library, because one timestamp format is not worth a dependency in a
/// program this size.
fn parse_rfc3339(s: &str) -> Option<SystemTime> {
    let s = s.trim();
    let bytes = s.as_bytes();
    if bytes.len() < 20 || bytes[4] != b'-' || bytes[7] != b'-' || (bytes[10] | 0x20) != b't' {
        return None;
    }
    let num = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, sec) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    // Offset, if any.
    let rest = &s[19..];
    let offset_seconds = if rest.starts_with('Z') || rest.starts_with('z') {
        0
    } else if let Some(sign_at) = rest.find(['+', '-']) {
        let sign = if rest.as_bytes()[sign_at] == b'-' {
            -1
        } else {
            1
        };
        let hh: i64 = rest.get(sign_at + 1..sign_at + 3)?.parse().ok()?;
        let mm: i64 = rest.get(sign_at + 4..sign_at + 6)?.parse().ok()?;
        sign * (hh * 3600 + mm * 60)
    } else if rest.starts_with('.') {
        // Fractional seconds then Z.
        0
    } else {
        return None;
    };
    let days = days_from_civil(y, mo, d);
    let epoch = days * 86_400 + h * 3600 + mi * 60 + sec - offset_seconds;
    if epoch < 0 {
        return None;
    }
    Some(UNIX_EPOCH + Duration::from_secs(epoch as u64))
}

/// Days from 1970-01-01 (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ts: &str) -> SystemTime {
        parse_rfc3339(ts).expect("a timestamp")
    }

    fn list(issued: &str, yanked: &[(&str, &str, &str)]) -> Denylist {
        Denylist {
            issued_at: issued.to_owned(),
            yanked: yanked
                .iter()
                .map(|(v, r, s)| Yanked {
                    version: (*v).to_owned(),
                    reason: (*r).to_owned(),
                    since: (*s).to_owned(),
                })
                .collect(),
        }
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn a_fresh_list_that_does_not_name_the_version_clears_it() {
        let l = list("2026-10-01T00:00:00Z", &[]);
        assert_eq!(
            l.verdict("0.1.0", at("2026-10-03T00:00:00Z")),
            Verdict::Clear
        );
        assert_eq!(
            decide(&Verdict::Clear, "0.1.0", no_env),
            Decision::Run {
                override_note: None
            }
        );
    }

    #[test]
    fn a_withdrawn_version_is_refused_with_the_day_and_the_reason() {
        let l = list(
            "2026-10-01T00:00:00Z",
            &[("0.1.0", "the bundle shipped a debug build", "2026-09-30")],
        );
        let verdict = l.verdict("0.1.0", at("2026-10-02T00:00:00Z"));
        let Decision::Refuse(message) = decide(&verdict, "0.1.0", no_env) else {
            panic!("a withdrawn version ran");
        };
        assert!(message.contains("withdrawn on 2026-09-30"), "{message}");
        assert!(message.contains("debug build"), "{message}");
        assert!(message.contains("Update the plugin."), "{message}");
    }

    #[test]
    fn a_stale_list_refuses_even_though_it_says_nothing_is_wrong() {
        // The whole point. The list is clean, and fifteen days old; the
        // answer is "I cannot confirm", not "nothing was withdrawn".
        let l = list("2026-09-01T00:00:00Z", &[]);
        let verdict = l.verdict("0.1.0", at("2026-10-03T00:00:00Z"));
        assert_eq!(
            verdict,
            Verdict::Unconfirmed {
                last_issued: Some("2026-09-01T00:00:00Z".into())
            }
        );
        let Decision::Refuse(message) = decide(&verdict, "0.1.0", no_env) else {
            panic!("a stale list let a version run");
        };
        assert!(message.contains("cannot confirm"), "{message}");
        assert!(message.contains("from 2026-09-01"), "{message}");
        assert!(message.contains(ENV_OFFLINE_OK), "{message}");
    }

    #[test]
    fn exactly_fourteen_days_is_still_fresh_and_a_minute_more_is_not() {
        let l = list("2026-09-19T00:00:00Z", &[]);
        assert_eq!(
            l.verdict("0.1.0", at("2026-10-03T00:00:00Z")),
            Verdict::Clear
        );
        assert!(matches!(
            l.verdict("0.1.0", at("2026-10-03T00:01:00Z")),
            Verdict::Unconfirmed { .. }
        ));
    }

    #[test]
    fn both_overrides_name_an_exact_version_and_announce_themselves() {
        let withdrawn = Verdict::Withdrawn {
            reason: "bad".into(),
            since: "2026-09-30".into(),
        };
        // The wrong version does not unlock it.
        assert!(matches!(
            decide(&withdrawn, "0.1.0", |k| (k == ENV_ALLOW_YANKED)
                .then(|| "0.0.9".to_owned())),
            Decision::Refuse(_)
        ));
        // The right one does, and says so.
        let Decision::Run { override_note } = decide(&withdrawn, "0.1.0", |k| {
            (k == ENV_ALLOW_YANKED).then(|| "0.1.0".to_owned())
        }) else {
            panic!("the override did not take");
        };
        let note = override_note.expect("an override is never silent");
        assert!(note.contains(ENV_ALLOW_YANKED), "{note}");
        assert!(note.contains("withdrawn on 2026-09-30"), "{note}");

        // And the offline override does not cover a withdrawal.
        assert!(matches!(
            decide(&withdrawn, "0.1.0", |k| (k == ENV_OFFLINE_OK)
                .then(|| "0.1.0".to_owned())),
            Decision::Refuse(_)
        ));
    }

    #[test]
    fn a_list_with_no_timestamp_is_not_a_list() {
        let e = Denylist::parse(br#"{"issued_at":"soon","yanked":[]}"#).unwrap_err();
        assert!(e.contains("not a timestamp"), "{e}");
        assert!(Denylist::parse(b"{}").is_err(), "issued_at is required");
    }

    #[test]
    fn a_list_issued_in_the_future_is_a_clock_problem_not_a_stale_list() {
        // A machine whose clock is a week behind would otherwise refuse
        // to start, and the signature already proves the list is ours.
        let l = list("2026-10-10T00:00:00Z", &[]);
        assert_eq!(
            l.verdict("0.1.0", at("2026-10-03T00:00:00Z")),
            Verdict::Clear
        );
    }

    #[test]
    fn timestamps_parse_the_shapes_a_publisher_writes() {
        assert!(parse_rfc3339("2026-10-03T21:00:00Z").is_some());
        assert!(parse_rfc3339("2026-10-03t21:00:00Z").is_some());
        assert!(parse_rfc3339("2026-10-03T21:00:00.123456Z").is_some());
        assert!(parse_rfc3339("2026-10-03T23:00:00+02:00").is_some());
        assert!(parse_rfc3339("2026-10-03").is_none());
        assert!(parse_rfc3339("not a date at all").is_none());
        // And the arithmetic is right.
        assert_eq!(
            parse_rfc3339("1970-01-02T00:00:00Z")
                .unwrap()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            86_400
        );
        // An offset is applied in the right direction.
        assert_eq!(
            parse_rfc3339("2026-10-03T02:00:00+02:00").unwrap(),
            parse_rfc3339("2026-10-03T00:00:00Z").unwrap()
        );
    }
}
