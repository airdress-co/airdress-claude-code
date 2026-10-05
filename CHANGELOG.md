# Changelog

Dates are the day a version was published. A version withdrawn after
publication stays listed here, and says so: the record of what happened
is more useful than a tidy list.

## Unreleased

- Shell sessions: hooks that, inside an Airdress shell session only,
  hand this session's events to the shell host on this machine through
  `airdress shell events`, and pass a permission answer tapped on your
  phone back to Claude Code (`PermissionRequest`, waiting up to 120 s,
  then nothing, so the terminal's own prompt stands). Outside a shell
  session every hook exits at once.
- First plugin: the query half and the functions dev loop, over MCP.
  Sign in, list airdresses, read one's status and capabilities; list
  functions, their versions and their logs; validate, deploy and promote
  a function; read and apply resources; read recent inbound events; and
  call the tools an airdress's own functions publish.
- A launcher that verifies before it runs: the pinned SHA-256, the
  Sigstore certificate identity, Rekor's inclusion proof, and the
  withdrawal list. Offline, against a trust root embedded in the plugin.
- Two download origins: our CDN first, which keeps no request record,
  then GitHub Releases.
- The agent bus: sessions, topics, messages with typed payloads, claims
  with fencing tokens, acknowledgements, handoff and shared state, every
  write signed by this machine's agent device. Other sessions' messages
  arrive as channel events and are always readable with `bus_read`.
  `/airdress:bus` and `/airdress:claim` use them.

### Known, and written down rather than smoothed over

- **The launcher is not yet reproducible across machines**, so no binary
  is committed. `sigstore`'s certificate verification depends on a C
  library (`aws-lc-sys`, or `ring` if you try to avoid it), and its
  object code follows the builder's C compiler. It is deterministic on
  one machine with one toolchain; the fix is a container pinned by
  digest, and that is not set up.
- **`rsa` carries RUSTSEC-2023-0071** (the Marvin Attack) with no safe
  upgrade, through `sigstore` → `openidconnect`. It is a timing
  side-channel in RSA *private-key* operations; this binary holds no
  private key and signs nothing, so there is no secret for it to leak.
  Recorded with that reasoning in `deny.toml` rather than silenced.
