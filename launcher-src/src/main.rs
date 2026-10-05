//! `airdress-launch` — verify the pinned MCP server bundle, then run it.
//!
//! An editor that loads a plugin runs whatever the plugin says to run,
//! and checks nothing about it. So the check has to happen here, before
//! the first byte executes, from a binary the marketplace pinned by
//! commit: hash, certificate identity, Rekor inclusion, withdrawal
//! list, and only then `exec`.
//!
//! What it prints, it prints to **stderr**. stdout belongs to the MCP
//! protocol from the moment the server starts, and a launcher that
//! wrote one line to it would break the session in a way that looks
//! like the server's fault.
//!
//! Everything it tells the server it tells through the environment:
//! which origin served the bundle, what was verified, and whether an
//! override is in force. The server reports those in `whoami`, so a
//! person can see how the thing they are talking to got there.

mod denylist;
mod fetch;
mod pins;
mod verify;

use std::path::{Path, PathBuf};
use std::process::Command;

use denylist::{Decision, Denylist};
use pins::Pins;

/// Run the named binary without verifying anything. For developing the
/// server itself; it says so on every start, and `whoami` repeats it.
const ENV_DEV_EXEC: &str = "AIRDRESS_MCP_DEV_EXEC";

/// What the launcher tells the server about its own provenance.
const ENV_ORIGIN: &str = "AIRDRESS_LAUNCH_ORIGIN";
/// See [`ENV_ORIGIN`].
const ENV_VERIFICATION: &str = "AIRDRESS_LAUNCH_VERIFICATION";
/// See [`ENV_ORIGIN`].
const ENV_OVERRIDE: &str = "AIRDRESS_LAUNCH_OVERRIDE";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("airdress-launch: {e}");
            std::process::exit(1);
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    // The development override, before anything else: somebody who sets
    // it is working on the server and does not want a download.
    if let Some(path) = std::env::var_os(ENV_DEV_EXEC) {
        let path = PathBuf::from(path);
        eprintln!(
            "airdress-launch: development override: {}, not verified",
            path.display()
        );
        return exec(
            &path,
            args,
            &[
                (ENV_ORIGIN, "development-override".to_owned()),
                (
                    ENV_VERIFICATION,
                    "none: a development override is in force".to_owned(),
                ),
                (
                    ENV_OVERRIDE,
                    format!(
                        "{ENV_DEV_EXEC}={} — this binary was not verified",
                        path.display()
                    ),
                ),
            ],
        );
    }

    let root = plugin_root()?;
    let pins_path = root.join("launcher/pins.json");
    if !pins_path.exists() {
        // A checkout of this repository between releases has no pins
        // file: the release workflow writes it, in a follow-up commit,
        // once the artifacts it pins exist. Say that, rather than
        // "cannot read a file".
        return Err(format!(
            "this plugin checkout pins no release yet ({} does not exist).\n\
             Install the plugin from the marketplace, which pins a released commit, or \
             set {ENV_DEV_EXEC}=<path to airdress-mcp> to run a local build.",
            pins_path.display()
        ));
    }
    let pins = Pins::read(&pins_path)?;
    let platform = pins.for_this_platform()?;
    let state = state_dir();

    // A cached bundle still re-hashes the binary it is about to run and
    // still re-checks the withdrawal list. What it does not do is make
    // a request for the bundle: that is the whole value of the cache,
    // and re-verifying costs milliseconds.
    let cache = state.join("bundles").join(&platform.sha256);
    let verified_marker = cache.join("verified");
    let cached = verified_marker.exists();

    let mut origin = "cache";
    if !cached {
        let fetched = fetch::get_from_origins(
            platform.bundle.in_order(),
            fetch::BUNDLE_TIMEOUT,
            Some(&platform.sha256),
        )
        .map_err(|f| format!("could not fetch the bundle from either origin:\n{f}"))?;
        origin = fetched.origin;

        let sigstore =
            fetch::get_from_origins(platform.sigstore.in_order(), fetch::BUNDLE_TIMEOUT, None)
                .map_err(|f| format!("could not fetch the signature from either origin:\n{f}"))?;

        verify::artifact(
            &fetched.bytes,
            &platform.sha256,
            &sigstore.bytes,
            &platform.cert_identity,
            &platform.cert_issuer,
        )
        .map_err(|e| format!("{e}"))?;

        extract(&fetched.bytes, &cache)?;
    }

    // The withdrawal list, every start, cached or not.
    let (verdict, note) = withdrawal_verdict(&pins, &state);
    if let Some(note) = note {
        eprintln!("airdress-launch: {note}");
    }
    let decision = denylist::decide(&verdict, &pins.version, |k| std::env::var(k).ok());
    let override_note = match decision {
        Decision::Refuse(message) => return Err(message),
        Decision::Run { override_note } => override_note,
    };
    if let Some(note) = &override_note {
        eprintln!("airdress-launch: {note}");
    }

    // The binary, re-hashed before it runs even from cache: a cache
    // directory is a file on a disk somebody else may also write to.
    let binary = cache.join("airdress-mcp");
    let bytes = std::fs::read(&binary)
        .map_err(|e| format!("the verified bundle has no runnable server: {e}"))?;
    let digest = fetch::sha256_hex(&bytes);
    let recorded = std::fs::read_to_string(cache.join("server.sha256"))
        .map_err(|e| format!("the cache has no recorded hash for the server: {e}"))?;
    if digest != recorded.trim() {
        // Refuse and clear it: the next start downloads again rather
        // than finding the same bad cache.
        let _ = std::fs::remove_file(&verified_marker);
        return Err(format!(
            "the cached server does not match what was verified\n  expected: {}\n  actual:   {digest}\n\
             The cache has been invalidated; start again.",
            recorded.trim()
        ));
    }

    let verification = format!(
        "sha256, certificate identity {}, Rekor inclusion, withdrawal list",
        platform.cert_identity
    );
    let mut env = vec![
        (ENV_ORIGIN, origin.to_owned()),
        (ENV_VERIFICATION, verification),
    ];
    if let Some(note) = override_note {
        env.push((ENV_OVERRIDE, note));
    }
    exec(&binary, args, &env)
}

/// Fetch the withdrawal list, falling back to the cached copy.
///
/// Returns the verdict and, where there is one, a line worth printing.
/// A fetch failure is not an error here: `Unconfirmed` is the verdict
/// and [`denylist::decide`] is what turns it into a refusal, so the
/// decision lives in one place.
fn withdrawal_verdict(pins: &Pins, state: &Path) -> (denylist::Verdict, Option<String>) {
    let now = std::time::SystemTime::now();
    let cache = denylist::cache_path(state);

    match fetch::get_from_origins(pins.denylist.list.in_order(), fetch::DENYLIST_TIMEOUT, None) {
        Ok(fetched) => {
            // Signed by its own workflow, not the release's: a
            // compromised release job must not be able to un-withdraw
            // itself.
            match fetch::get_from_origins(
                pins.denylist.sigstore.in_order(),
                fetch::DENYLIST_TIMEOUT,
                None,
            ) {
                Ok(sig) => {
                    let digest = fetch::sha256_hex(&fetched.bytes);
                    if let Err(e) = verify::artifact(
                        &fetched.bytes,
                        &digest,
                        &sig.bytes,
                        &pins.denylist.cert_identity,
                        &pins.denylist.cert_issuer,
                    ) {
                        return (
                            cached_verdict(&cache, &pins.version, now),
                            Some(format!("the fetched withdrawal list did not verify: {e}")),
                        );
                    }
                    match Denylist::parse(&fetched.bytes) {
                        Ok(list) => {
                            let _ = std::fs::create_dir_all(state);
                            let _ = std::fs::write(&cache, &fetched.bytes);
                            (list.verdict(&pins.version, now), None)
                        }
                        Err(why) => (
                            cached_verdict(&cache, &pins.version, now),
                            Some(format!("the fetched withdrawal list was unreadable: {why}")),
                        ),
                    }
                }
                Err(_) => (cached_verdict(&cache, &pins.version, now), None),
            }
        }
        Err(_) => (cached_verdict(&cache, &pins.version, now), None),
    }
}

fn cached_verdict(cache: &Path, version: &str, now: std::time::SystemTime) -> denylist::Verdict {
    match std::fs::read(cache)
        .ok()
        .and_then(|b| Denylist::parse(&b).ok())
    {
        Some(list) => list.verdict(version, now),
        None => denylist::Verdict::Unconfirmed { last_issued: None },
    }
}

/// Unpack a verified bundle, and record what the server hashes to.
fn extract(bytes: &[u8], into: &Path) -> Result<(), String> {
    let staging = into.with_extension("staging");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| format!("cannot write the cache: {e}"))?;

    let reader = std::io::Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| format!("the bundle is not an archive: {e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("the bundle is damaged: {e}"))?;
        // A path from an archive decides where a write lands, so it is
        // not trusted: no absolute paths, no `..`, no symlinks.
        let name = entry.name().to_owned();
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| format!("the bundle holds an unsafe path: {name}"))?;
        let target = staging.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| format!("cannot write {name}: {e}"))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("cannot write {name}: {e}"))?;
        }
        let mut out =
            std::fs::File::create(&target).map_err(|e| format!("cannot write {name}: {e}"))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| format!("cannot write {name}: {e}"))?;
        #[cfg(unix)]
        if name.ends_with("airdress-mcp") {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| format!("cannot make the server executable: {e}"))?;
        }
    }

    let server = staging.join("airdress-mcp");
    let server_bytes =
        std::fs::read(&server).map_err(|e| format!("the bundle holds no airdress-mcp: {e}"))?;
    std::fs::write(
        staging.join("server.sha256"),
        fetch::sha256_hex(&server_bytes),
    )
    .map_err(|e| format!("cannot record the server's hash: {e}"))?;

    // `verified` is written last, and only here. Its presence is what a
    // later start reads as "this directory was checked".
    std::fs::write(staging.join("verified"), "ok")
        .map_err(|e| format!("cannot mark the cache verified: {e}"))?;
    let _ = std::fs::remove_dir_all(into);
    if let Some(parent) = into.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("cannot write the cache: {e}"))?;
    }
    std::fs::rename(&staging, into).map_err(|e| format!("cannot publish the cache: {e}"))?;
    Ok(())
}

/// Replace this process with the server.
///
/// `exec` rather than spawn-and-wait: the server owns stdin and stdout
/// from here, and a launcher sitting in the middle of a protocol stream
/// is a launcher that can corrupt it.
fn exec(binary: &Path, args: &[String], env: &[(&str, String)]) -> Result<(), String> {
    let mut command = Command::new(binary);
    command.args(args);
    for (key, value) in env {
        command.env(key, value);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        let error = command.exec();
        Err(format!("could not run {}: {error}", binary.display()))
    }
    #[cfg(not(unix))]
    {
        let status = command
            .status()
            .map_err(|e| format!("could not run {}: {e}", binary.display()))?;
        std::process::exit(status.code().unwrap_or(1));
    }
}

/// The plugin directory this launcher was run from.
///
/// `CLAUDE_PLUGIN_ROOT` when the harness set it; otherwise the parent
/// of this binary's directory, which is what the shell wrapper's layout
/// gives. Named by the environment variable the harness uses, which is
/// the one place in this repository that is specific to one editor —
/// and this repository is the one place in the product that may be.
fn plugin_root() -> Result<PathBuf, String> {
    if let Some(root) = std::env::var_os("CLAUDE_PLUGIN_ROOT") {
        return Ok(PathBuf::from(root));
    }
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate myself: {e}"))?;
    // …/launcher/<platform>/airdress-launch → …/
    exe.parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| "cannot locate the plugin directory".to_owned())
}

/// Where the cache and the withdrawal list live.
fn state_dir() -> PathBuf {
    for key in ["CLAUDE_PLUGIN_DATA", "AIRDRESS_LAUNCH_STATE_DIR"] {
        if let Some(dir) = std::env::var_os(key) {
            let dir = PathBuf::from(dir);
            if !dir.as_os_str().is_empty() {
                return dir;
            }
        }
    }
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    home.join(".cache/airdress-launch")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_directory_prefers_what_the_harness_gave_it() {
        // Set and read in one test, because the environment is
        // process-global and two tests racing on it would flake.
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("AIRDRESS_LAUNCH_STATE_DIR", dir.path());
        std::env::remove_var("CLAUDE_PLUGIN_DATA");
        assert_eq!(state_dir(), dir.path());
        std::env::set_var("CLAUDE_PLUGIN_DATA", "/tmp/from-the-harness");
        assert_eq!(state_dir(), PathBuf::from("/tmp/from-the-harness"));
        std::env::remove_var("CLAUDE_PLUGIN_DATA");
        std::env::remove_var("AIRDRESS_LAUNCH_STATE_DIR");
    }

    /// A `.mcpb` bundle holding one named file.
    fn bundle(entries: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write as _;
        let mut buffer = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
            for (name, body) in entries {
                zip.start_file::<_, ()>(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(body).unwrap();
            }
            zip.finish().unwrap();
        }
        buffer
    }

    #[test]
    fn extracting_records_the_servers_hash_and_marks_the_cache_last() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("bundles/abc");
        let server = b"#!/bin/sh\nexit 0\n";
        extract(
            &bundle(&[("airdress-mcp", server), ("README", b"hello")]),
            &cache,
        )
        .unwrap();

        assert!(cache.join("airdress-mcp").exists());
        assert!(cache.join("README").exists());
        assert_eq!(
            std::fs::read_to_string(cache.join("server.sha256")).unwrap(),
            fetch::sha256_hex(server)
        );
        assert!(
            cache.join("verified").exists(),
            "the marker is the contract"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(cache.join("airdress-mcp"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(
                mode & 0o777,
                0o700,
                "the server is not executable by us only"
            );
        }
    }

    /// The release packs bundles STORED, not deflated, so that two builds
    /// produce one archive whatever zlib each runner has
    /// (`scripts/package-mcpb.py`). The extraction has to take that form.
    #[test]
    fn a_stored_bundle_as_the_release_packs_it_extracts() {
        use std::io::Write as _;
        let mut buffer = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
            let stored = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored)
                .unix_permissions(0o755);
            for (name, body) in [
                ("README.md", &b"readme"[..]),
                ("airdress-mcp", &b"#!/bin/sh\nexit 0\n"[..]),
                ("manifest.json", &b"{}"[..]),
            ] {
                zip.start_file::<_, ()>(name, stored).unwrap();
                zip.write_all(body).unwrap();
            }
            zip.finish().unwrap();
        }
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("bundles/stored");
        extract(&buffer, &cache).unwrap();
        assert!(cache.join("airdress-mcp").exists());
        assert!(cache.join("manifest.json").exists());
        assert!(cache.join("verified").exists());
    }

    #[test]
    fn a_bundle_without_a_server_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let e = extract(
            &bundle(&[("something-else", b"x")]),
            &dir.path().join("cache"),
        )
        .unwrap_err();
        assert!(e.contains("no airdress-mcp"), "{e}");
    }

    #[test]
    fn a_bundle_that_tries_to_escape_the_cache_is_refused() {
        // `zip`'s own writer will not emit `..`, so the entry is built
        // by hand through its raw name.
        let mut buffer = Vec::new();
        {
            use std::io::Write as _;
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
            zip.start_file::<_, ()>("../escaped", zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"x").unwrap();
            zip.finish().unwrap();
        }
        let dir = tempfile::tempdir().unwrap();
        let e = extract(&buffer, &dir.path().join("cache")).unwrap_err();
        assert!(e.contains("unsafe path"), "{e}");
        assert!(
            !dir.path().join("escaped").exists(),
            "a path from an archive wrote outside the cache"
        );
    }

    #[test]
    fn a_damaged_archive_is_refused_rather_than_half_extracted() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache");
        assert!(extract(b"not a zip file at all", &cache).is_err());
        assert!(!cache.exists(), "a failed extraction left a cache behind");
    }
}
