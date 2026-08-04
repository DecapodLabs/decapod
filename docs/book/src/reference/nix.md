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

When maintainers bump `rust-toolchain.toml`, they must refresh and commit
`flake.lock`’s `rust-overlay` input (`nix flake update rust-overlay`). CI
detects a stale overlay and does not rewrite the lock.

Full support-matrix notes and the toolchain-bump checklist live in
[CONTRIBUTING.md](https://github.com/DecapodLabs/decapod/blob/master/CONTRIBUTING.md#nix-packaging-maintainers--packagers).
