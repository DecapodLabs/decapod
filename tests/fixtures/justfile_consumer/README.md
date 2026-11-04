# Rust consumer for source-spelling acceptance

This dependency-free Rust crate has exactly one intended tracked `Justfile`.
Its recipe runs `cargo test --locked`. The distance and temperature modules
provide separate task-bound artifacts for future historical recovery acceptance.

`tests/support/justfile_inventory.rs` embeds this fixture into a disposable Git
repository and checks real `init` / `agent.init` inventory against the Git index.
The required `cli_contracts` suite compiles and runs the fixture's own unit tests.
Its macOS-specific case additionally requires APFS and verifies that both case
spellings identify the same inode; an unsuitable filesystem fails that test.

These checks establish source inventory binding and fixture correctness. They do
not establish full `repo.pr-readiness.v1`, publication readiness, or successful
historical task recovery. Live acceptance still needs an authorized governed
consumer with an independently accepted target, reviewed native proof contract
and actual execution evidence. This fixture contains no datastore, credentials,
session state, accepted receipt, or substitute proof provider.
