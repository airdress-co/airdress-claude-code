# Airdress for Claude Code

Your airdresses, from your editor. List them, read what one is doing,
and run your functions dev loop — validate, deploy, promote — without
leaving the session you are already in.

```text
/plugin marketplace add airdress-co/claude-plugin
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
| `/airdress:bus` | The agent bus (a later release) |
| `/airdress:claim <name>` | Claim a shared task (a later release) |

The tools behind them: `login`, `whoami`, `logout`;
`airdresses_list`, `airdress_status`; `function_list`,
`function_versions`, `function_logs`, `function_templates`,
`function_validate`, `function_deploy`, `function_promote`;
`resources_list`, `resources_get`, `resources_apply`;
`ingress_events_recent`; `bridge_list`, `bridge_call`, and every tool
your own functions publish, re-exported as `fn_<tool>`.

**Read only**, in the plugin's settings, removes every tool that changes
anything — and a model that asks for one by name is refused, rather than
quietly obliged.

## What it is not, yet

This release is the query half. The agent bus, agent devices and agent
chat are designed and not built; their commands say so rather than
failing oddly. An airdress can also have the editor path switched off,
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
profile and writes no second copy of any token. Nothing it returns — a
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
    "https://github.com/airdress-co/claude-plugin/.github/workflows/release.yml@refs/tags/v0.1.0" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  airdress-linux-x86_64.mcpb
```

The launcher binaries are committed to this repository, per platform.
That is deliberate: the marketplace pins a commit, which is what makes
them trustworthy, and CI rebuilds them from `launcher-src/` and refuses
a commit whose binaries differ.

### The two download origins

| Origin | What it is |
| -------- | ----------- |
| `downloads.airdress.co/claude-plugin/…` | Tried first. **We keep no request record of this path** — load-balancer logging and storage access logs for it are switched off and a scheduled check looks for any entry that appears anyway. |
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
| Join the agent bus | off | For a later release |
| Bus topics | `general` | For a later release |
| Session label | `<host> · <repo>` | How other sessions see this one |

## Contributing, and reporting something

The server is in
[airdress-cli](https://github.com/airdress-co/airdress-cli); this
repository is the plugin and its launcher. Security reports:
[SECURITY.md](./SECURITY.md).

Apache-2.0. "Claude" and "Claude Code" are trademarks of Anthropic, PBC;
this plugin is published by Airdress Co. and is not affiliated with or
endorsed by Anthropic.
