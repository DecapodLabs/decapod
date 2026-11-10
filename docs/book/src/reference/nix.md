# Nix packaging

Decapod’s primary install path is Cargo (`cargo binstall decapod`). The
repository also ships a Nix flake for packagers and Nix-native users.

```bash
nix run github:DecapodLabs/decapod -- init
nix build github:DecapodLabs/decapod    # ./result/bin/decapod
nix develop github:DecapodLabs/decapod  # optional shell
```

The package derives its crate graph from the committed `Cargo.lock` and builds
with the repository-pinned Rust channel through the locked `rust-overlay`
input. Continuous CI proves native **`x86_64-linux`** and **`aarch64-darwin`**
builds (including a short `decapod system version` smoke). Other flake systems
may evaluate without continuous proof.

Both packaging and publication smoke checks execute the built binary by its
absolute checkout path from a fresh private temporary directory, removed on
success or failure. This keeps checkout-local `.decapod` state and shared CI
checkout permissions outside the package smoke check; runtime storage permission
checks remain enforced.

The build is reproducible on `x86_64-linux`: two builds from the same locked
inputs give byte-identical output, and CI checks this on each release merge.
To check a binary that you got from a cache
or a mirror, rebuild it locally and let Nix compare:

```bash
nix build github:DecapodLabs/decapod --rebuild
```

Two things keep it reproducible (#1176). Build-time code generation must not
depend on `HashMap` iteration order: the constitution build script
(`assets/build/compress_constitution.rs`) serializes `BTreeMap`s, whose order
is fixed. And the flake maps the build directory to `/build`, because on Darwin
it is a random path that rustc would embed in panic locations.

On `aarch64-darwin` one difference can remain. With upstream Nix, two builds
can differ in the Mach-O `LC_UUID` and in the one code-signature page hash
that covers it; every other byte is the same. ld64 computes the UUID before
`strip -S`, over linker output whose symbol tables still depend on the build
directory. Lix gives byte-identical rebuilds. `-Wl,-S` and `-Wl,-reproducible`
do not remove the difference.

## Binary cache

On native GitHub Actions, release merges publish `packages.default` for
`x86_64-linux` and `aarch64-darwin` to the public `decapod` Cachix cache.
Publishing requires a write-capable `CACHIX_AUTH_TOKEN` repository secret;
when it is missing or rejected, the publishing job fails rather than reporting
a false success. Nix can then download the binary instead of rebuilding it:

```bash
cachix use decapod
```

Without the `cachix` command, add `https://decapod.cachix.org` to
`extra-substituters`, and the public key that
<https://app.cachix.org/cache/decapod> shows to `extra-trusted-public-keys`.

Only a release merge on `master` publishes. The credential-free owner job
checks the native GitHub runner, push event and release merge identity before
GitHub schedules the separate secret-bearing publisher. Buildkite-imported
jobs, manual runs, ordinary merges and pull requests cannot publish. The
publisher rebuilds natively from the committed `flake.lock` and verifies the
Rust toolchain, smoke-check and Linux reproducibility before uploading; a
separate credential-free proof job must also succeed. On `x86_64-linux` a rebuild must also be
byte-identical, so you can check a cached binary yourself with
`nix build --rebuild`. On `aarch64-darwin` that check can fail on the linker
UUID, so for Darwin you trust the CI builder and the cache's signing key.

When maintainers bump `rust-toolchain.toml`, they must refresh and commit
`flake.lock`’s `rust-overlay` input (`nix flake update rust-overlay`). CI
detects a stale overlay and does not rewrite the lock.

Full support-matrix notes and the toolchain-bump checklist live in
[CONTRIBUTING.md](https://github.com/DecapodLabs/decapod/blob/master/CONTRIBUTING.md#nix-packaging-maintainers--packagers).
