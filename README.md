# Airdress for Claude Code

Your airdresses, from your editor. List them, read what one is doing,
and run your functions dev loop — validate, deploy, promote — without
leaving the session you are already in.

```text
/plugin marketplace add airdress-co/airdress-claude-code
/plugin install airdress@airdress
/airdress:login
```

Linux and macOS. On Windows, connect over
[remote MCP](https://airdress.co/docs/claude) instead — the local path
needs a device host and a keychain this plugin does not have there yet,
and pretending otherwise would fail halfway through a session.

## What it gives you

| Command | What it does |
| --------- | -------------- |
| `/airdress:login` | Sign in |
| `/airdress:status` | Account, airdress, what it has turned on, how this server was verified |
| `/airdress:fn [name]` | Functions: list, versions, logs, deploy |
| `/airdress:bus` | The agent bus: who is connected, topics, unread |
| `/airdress:claim <name> [topic]` | Claim a shared task before working on it |

The tools behind them: `login`, `whoami`, `logout`;
`airdresses_list`, `airdress_status`; `function_list`,
`function_versions`, `function_logs`, `function_templates`,
`function_validate`, `function_deploy`, `function_promote`;
`resources_list`, `resources_get`, `resources_apply`;
`ingress_events_recent`; `bridge_list`, `bridge_call`, and every tool
your own functions publish, re-exported as `fn_<tool>`.

The agent bus: `bus_sessions`, `bus_topics`, `bus_read`, `bus_thread`,
`bus_claims`, `bus_state_get`, `bus_state_list`, `bus_topic_policy`;
`bus_post`, `bus_reply`, `bus_ack`; `bus_claim`, `bus_renew`,
`bus_release`, `bus_handoff`; `bus_state_put`, `bus_state_delete`.
Every write is signed by this machine's agent device, and every item is
labelled **device-signed** or **operator-attested** (written from a
hosted assistant and vouched for by the server, which is the weaker
promise), with whether its signature verified. Other sessions' messages
arrive in the conversation as channel events when the Airdress channel
is on, and are always readable with `bus_read`, so nothing is lost when
it is off. The bus relays no permission prompts.

**Inside an Airdress shell session**, and only there, the plugin's hooks
report this session's events (prompts, tool calls, the end of a turn) to
the shell host on this machine, which shows them end to end encrypted on
your own devices. A permission prompt can then be answered by your tap
on your phone: the hook waits up to two minutes for it, and with no
answer the prompt in your terminal stands. Nothing answers for you.
Outside a shell session (`airdress shell run`, or a profile your shell
host starts), every hook exits at once and does nothing. The hooks run
`airdress shell events` from the `airdress` CLI, which must be on your
`PATH`; they read nothing of Claude Code's own files.

**Read only**, in the plugin's settings, removes every tool that changes
anything — and a model that asks for one by name is refused, rather than
quietly obliged.

## What it is not, yet

This release is the query half and the agent bus. Agent chat is
designed and not built. Writing to the bus needs this machine approved
as an agent device (`airdress agent device join`); without one, the bus
can be read and not written, and the tools say so. An airdress can also have the editor path switched off,
and then every tool answers one sentence saying so, with a link to the
page where that is decided.

Nothing here is end-to-end encrypted, because nothing here carries
message content: it reads and writes your airdress's own API over TLS,
as the `airdress` CLI does.

## What leaves your machine

Your hub, your airdresses' operators, and — for the plugin's own
updates — two download origins. Nothing else. There is no analytics, no
crash reporting and no tracing exporter: the crates that would do it are
banned in the build, and a test drives every tool with every other host
unreachable.

What the model provider sees is what any model provider sees: the
conversation, including whatever a tool returned into it. If you ask for
your function's logs, those logs are in the conversation. That is worth
knowing before you ask for something sensitive, and it is not something
a plugin can change.

**No credential ever leaves.** The server borrows your `airdress` CLI
profile and writes no second copy of any token. With a current sign-in
(`airdress auth login`, which signs in through the hub) each airdress is
sent a token only that airdress accepts. A profile still holding the
older sign-in, one token every airdress accepts, keeps working, and the
server says so in the first answer of every session until you sign in
again. Nothing it returns — a
result, an error, a log line — carries an access token, a refresh token,
a device code or a signing key, and a test hands it real-shaped secrets
and fails if one appears.

## Verifying what runs

Claude Code runs what a plugin says to run and checks nothing about it.
So the check happens first, in a launcher this repository pins by commit.
Before any byte of the server executes:

1. the bundle's SHA-256 matches the pin in this commit's
   `launcher/pins.json`;
2. its signature's certificate chains to Fulcio and was valid when it
   signed;
3. the certificate's identity is **exactly** this repository's release
   workflow at that tag;
4. Rekor's inclusion proof checks out, offline, against a trust root
   embedded in the plugin;
5. the version is not on the signed withdrawal list.

Any failure prints the check, the expected value and the actual one, and
refuses. `/airdress:status` reports what was verified and which origin
served it.

Check it yourself, from either origin:

```sh
cosign verify-blob \
  --bundle airdress-linux-x86_64.mcpb.sigstore.json \
  --certificate-identity \
    "https://github.com/airdress-co/airdress-claude-code/.github/workflows/release.yml@refs/tags/v0.1.0" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  airdress-linux-x86_64.mcpb
```

The launcher is built from `launcher-src/` in this repository, and the
marketplace pins the commit it is built from.

### Rebuilding the launcher yourself

All three launchers are committed, and you can check them:

```sh
scripts/build-launchers.sh          # Linux; needs docker, podman or nerdctl
scripts/build-launcher-macos.sh     # macOS; needs Xcode 26.4
```

It builds inside a toolchain image pinned by digest in
`launcher-src/toolchain-images.json`, and the result should be
byte-identical to what is committed. CI does the same on every pull
request and compares against hashes produced on a different machine, so
a build that only reproduces in one place fails.

| Platform | SHA-256 | Reproduces |
| --- | --- | --- |
| `linux-x86_64` | `18ac3d9adcbb3fb0098965917890ede9c0134d584ce84d7308ca1b03512f1175` | yes, in its pinned image |
| `linux-aarch64` | `c7f780812b52524cae0171f2d38aea43f001a50d352cb05766f082df613b4277` | yes, in its pinned image |
| `darwin-universal` | `7d2de6bd3af9b027d0c43ce3598022179b10a7c68ab0d85d3ab466725998ac7a` | only with Xcode 26.4 — weaker than an image: two machines agreeing, not a pinned toolchain |

**Why the C toolchain has to be pinned.** `sigstore`'s certificate
verification reaches a C library (`aws-lc-sys`) by three independent
routes, and a C library's object code depends on whichever compiler
built it — so pinning `rustc` alone is not enough, and two machines with
different `cc` produce different binaries. Measured: identical twice on
one machine, different on a CI runner. The image pins `cc`; `rustc`
still comes from `rust-toolchain.toml`, so the image's own Rust version
is irrelevant.

**What that claim is, exactly.** These bytes are what *this toolchain
image* produces from this source, on any machine that can run it. It is
not a claim that the toolchain itself is derivable from source: it rests
on the registry still serving that digest. That is weaker than full
bootstrappable provenance and stronger than "trust our CI", and it is
worth saying which one it is.

**macOS gets weaker evidence, and that is a real gap.** Apple's
toolchain cannot be pinned by digest, so `darwin-universal` cannot get
the Linux guarantee. What it has instead: the hash above was produced on
one Mac, twice — the second time from another checkout path with a cargo
home of its own — and CI rebuilds it separately on two macOS runner
versions with the same Xcode selected by name, and fails unless both
match it. The
Xcode is the pin that remains — its compiler builds `aws-lc`'s C and its
SDK version is written into the binary — so another Xcode is expected to
give another hash, and that is a toolchain change, not a fault.

The macOS launcher is one universal binary (`x86_64` and `arm64`) with
the linker's ad-hoc signature and **no Developer ID signature**. A
plugin installed through Claude Code is not quarantined, so Gatekeeper
never assesses it and it runs as committed. Copied out of a browser
download instead, it would be quarantined, and macOS would stop it with
a dialog that an editor running it in the background never shows — so
install it through the marketplace, or clear the attribute with
`xattr -d com.apple.quarantine` after checking the hash.

### The two download origins

| Origin | What it is |
| -------- | ----------- |
| `downloads.airdress.co/claude-code/…` | Tried first. **We keep no request record of this path** — load-balancer logging and storage access logs for it are switched off and a scheduled check looks for any entry that appears anyway. |
| GitHub Releases | The fallback and the public record. **GitHub keeps its own request logs**, which is the one place outside our control where a download of this plugin is recorded. |

Neither is trusted. The hash, the signature identity and the inclusion
proof decide; an origin can deny you a download, and cannot give you a
different one. Launcher requests carry a constant `User-Agent`, no query
string, no cookie, no `Referer` and no custom header, and a test runs
the launcher against a capture server and fails on anything else.

### If verification cannot happen

The launcher accepts a cached withdrawal list for **fourteen days** from
its `issued_at`, and then refuses to start. That is on purpose: a
program that runs whatever it has when it cannot check is a program an
attacker makes unable to check, and blocking a hostname is easier than
forging a signature. Fourteen days is longer than a holiday offline and
shorter than a release cycle.

Three escape hatches, each naming an exact version and each printed on
every start:

```sh
AIRDRESS_MCP_OFFLINE_OK=0.1.0     # no current withdrawal list; accept the risk
AIRDRESS_MCP_ALLOW_YANKED=0.1.0   # run a version that was withdrawn
AIRDRESS_MCP_DEV_EXEC=/path/to/airdress-mcp   # run a local build, unverified
```

## Settings

| Setting | Default | What it does |
| --------- | --------- | -------------- |
| CLI profile | the active one | Which `airdress` profile to act as |
| Default airdress | resolved | Which airdress a tool acts on when it names none |
| Read only | off | Remove every tool that changes anything |
| Agent chat | on | For a later release |
| Join the agent bus | off | Register this session on the bus at start |
| Bus topics | `general` | The topics this session joins |
| Session label | `<host> · <repo>` | How other sessions see this one |

## Contributing, and reporting something

The server is in
[airdress-cli](https://github.com/airdress-co/airdress-cli); this
repository is the plugin and its launcher. Security reports:
[SECURITY.md](./SECURITY.md).

Apache-2.0. "Claude" and "Claude Code" are trademarks of Anthropic, PBC;
this plugin is published by Airdress Co. and is not affiliated with or
endorsed by Anthropic.
