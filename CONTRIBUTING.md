# Contributing

The server lives in
[airdress-cli](https://github.com/airdress-co/airdress-cli). This
repository holds the plugin, its launcher, and the pins that say which
server the launcher may run. Security reports go to
[SECURITY.md](./SECURITY.md).

## Landing a change

Branch, commit with a conventional commit message
(`scripts/check-conventional-commit.py` checks it), push the branch and
open a pull request. A maintainer lands it as a rebase merge; this
repository allows no merge commits and no squash:

```sh
gh pr merge <pr> --repo airdress-co/airdress-claude-code --rebase --admin --delete-branch
```

## Changing the launcher

The committed launcher binaries have to be what their source builds, and
CI rebuilds them to check. Any change under `launcher-src/` therefore
ships its binaries in the same pull request, and that includes a
`Cargo.lock` bump made for an advisory.

- **Linux**: `scripts/build-launchers.sh --write` builds both platforms in
  the digest-pinned images named in `launcher-src/toolchain-images.json`
  (Docker or Podman). Copy the hashes it prints into the `expected` table
  in `.github/workflows/ci.yml` and into the table in the README.
- **macOS**: the binary is signed and notarized, so it cannot be built
  locally. Dispatch the signing workflow on your branch:

  ```sh
  gh workflow run sign-macos-launcher.yml --repo airdress-co/airdress-claude-code --ref <branch>
  gh run download <run> -n airdress-launch-signed
  install -m 0755 airdress-launch plugins/airdress/launcher/darwin-universal/
  ```

  A person commits that binary; the signing secrets never get a token that
  can write to the repository. Update the darwin hash in the README in the
  same commit. CI then compares it with a fresh build, signature aside, on
  two macOS runners.

If a pull request's checks still show an older commit after a push, GitHub
has not synced the pull request yet. Closing and reopening it forces the
sync, but a pull request whose branch was force-pushed may then refuse to
reopen. A new pull request from the same branch is the way out.

## Releasing

A plugin release takes three or four pull requests and one tag, in this
order. The pins name the hashes of published artifacts, so they can only
be written once the release exists.

1. **Move the server, if it changed.** Set `tag` and `commit` in
   `server-src.json` to an airdress-cli release
   (`git rev-list -n1 vX.Y.Z` in airdress-cli gives the commit), and add a
   sentence to its `comment` saying what the move brings. The server build
   stops if the tag ever resolves to another commit.
2. **The release commit**, `chore: release X.Y.Z`. Set `version` in
   `plugins/airdress/.claude-plugin/plugin.json` and the plugin's entry in
   `.claude-plugin/marketplace.json`. Step 1 can be in the same commit;
   v0.1.5 was. Land it.
3. **Tag it.** Make an annotated tag `vX.Y.Z`, equal to `plugin.json`'s
   version, on that commit on `main`, signed by a person (`git tag -s`), and
   push it. `.github/workflows/release.yml` runs on the tag. It builds each
   bundle twice and compares them, signs and notarizes macOS, signs every
   bundle with Sigstore, writes the SLSA provenance, and publishes to
   GitHub Releases and `downloads.airdress.co/claude-code/`. The
   certificate names this workflow at the tag, so a run from a branch
   signs bundles no launcher accepts.
4. **Pin the bundles**, `chore(launcher): pin the vX.Y.Z bundles`. Run
   `scripts/write-pins.sh X.Y.Z`, which writes
   `plugins/airdress/launcher/pins.json`, then land it.
5. **Pin the marketplace**, `chore(marketplace): pin the plugin to the
   vX.Y.Z pins commit`. Run `scripts/marketplace-pins.py pin <the commit
   from step 4 on main>`, then land it. Until this lands, a marketplace
   install still gets the previous release.

From airdress-ops, `just claude-code-verify X.Y.Z` checks that both origins
serve the same verified bytes.

A release whose signature fails Rekor's inclusion check in the launcher
usually means a trust root older than the signing log. Run
`scripts/refresh-trust-root.sh`, review the diff, and release again.
