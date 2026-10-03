# Changelog

Dates are the day a version was published. A version withdrawn after
publication stays listed here, and says so: the record of what happened
is more useful than a tidy list.

## Unreleased

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
