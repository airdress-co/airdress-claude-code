# Security policy

## Reporting a vulnerability

Email **<security@airdress.co>**. Please include what you did, what
happened, and what you expected.

We answer within three working days and tell you plainly whether we
consider it a vulnerability, with the reason either way. We do not run a
bug bounty. Please give us 90 days before publishing, and tell us if you
intend to publish sooner.

## What this repository is

A plugin: a manifest, five commands, and a launcher that verifies and
runs the Airdress MCP server. The server itself lives in
[airdress-cli](https://github.com/airdress-co/airdress-cli) and has its
own policy.

## What is in scope here

The launcher is the whole security surface of this repository, because
it decides whether anything runs:

- Anything that makes it execute a binary it has not verified: a hash it
  accepts that does not match, a certificate identity it does not check,
  an inclusion proof it skips, a withdrawal it ignores.
- Anything that makes it contact a host other than the two download
  origins, or carry anything identifying in a request.
- Anything that writes outside its cache directory — an archive entry
  that escapes it, for instance.
- A withdrawal list it accepts that is older than fourteen days, or that
  is not signed by the withdrawal workflow's own identity.

## What is not a vulnerability

Three environment variables weaken verification on purpose. Each names
what it is doing on every single start, and `whoami` repeats it:

- `AIRDRESS_MCP_DEV_EXEC` runs a named binary without verifying it.
- `AIRDRESS_MCP_ALLOW_YANKED=<version>` runs a withdrawn version.
- `AIRDRESS_MCP_OFFLINE_OK=<version>` runs without a current withdrawal
  list.

Somebody who can set an environment variable in your shell can already
run their own program. Both version-named overrides take an exact
version, so one set and forgotten cannot cover a release nobody
considered.

GitHub keeps request logs for its own downloads. That is stated in the
README, and it is why our CDN is tried first.
