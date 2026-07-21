# Error Handling

Decapod uses Rust's `Result<T, DecapodError>` contract internally. Commands
return errors to the process boundary, where Decapod prints a human-readable
message on stderr and exits unsuccessfully. The error variant is useful for
the message and recovery guidance; it does not currently select a distinct
process exit status.

## Process Exit Status

| Status | Meaning | Description |
|---|---|---|
| 0 | Success | The operation completed successfully. |
| 1 | Decapod operation failure | A domain, validation, configuration, session, I/O, or storage error was returned. `decapod validate` also uses status 1 when a gate fails. |
| 2 | CLI syntax failure | Clap rejected an unknown command, argument, or option before Decapod ran the operation. |
| 127 | Shell command not found | This is normally emitted by the calling shell when `decapod` or another command is absent; Decapod does not use 127 for its Rust errors. |

Do not infer a unique status from `DecapodError::Config`, `NotFound`, or
`SessionError`: all of those domain errors currently use status 1. Scripts
that need to distinguish failures should inspect the command output or use a
structured command surface rather than depend on the error text as a stable
API.

## Idiomatic Rust Error Handling

Functions that can fail return `Result` and use `?` to propagate errors. Use
`map_err` when a lower-level error needs to be translated into Decapod's
domain error type or given application context:

```rust
fn load_config(path: &Path) -> Result<Config, DecapodError> {
    let text = std::fs::read_to_string(path)?;
    toml::from_str(&text).map_err(|error| DecapodError::Config(error.to_string()))
}

fn run(path: &Path) -> Result<(), DecapodError> {
    let config = load_config(path)?;
    validate_config(&config)
        .map_err(|message| DecapodError::ValidationError(message))?;
    Ok(())
}
```

The `?` operator preserves the original error when a `From` conversion exists
(for example, `std::io::Error` becomes `DecapodError::IoError`). `map_err` is
appropriate when the caller needs a different domain variant or additional
context. Avoid `unwrap` and `expect` for user input, files, environment
variables, or external commands; reserve them for proven invariants and test
fixtures.

At the CLI boundary, `src/main.rs` matches the result from `decapod::run()`,
prints the error with its `Display` implementation, and exits with status 1.
That keeps error propagation explicit without exposing Rust's debug-style
enum representation to command-line users.

## Error Variants and Recovery

`DecapodError` currently contains these variants:

- `RusqliteError`: inspect the database path, schema, and lock state.
- `IoError`: check the referenced path, permissions, and external process.
- `DatabaseInitializationError`: inspect repository initialization and schema setup.
- `PathError`: correct the repository, workspace, or artifact path.
- `EnvVarError`: check the required environment variable and its encoding.
- `ValidationError`: follow the reported gate and remediation command.
- `NotFound`: verify the requested task, workspace, document, or artifact ID/path.
- `NotImplemented`: use a supported command or defer the operation.
- `Config`: correct `.decapod/config.toml` or the relevant environment setting.
- `ContextPackError`: inspect the context pack and its integrity metadata.
- `SessionError`: run `decapod session acquire`, then retry the operation.

The wrapped SQLite, I/O, and environment errors remain available through the
standard `std::error::Error::source` chain, so callers can log the underlying
cause without parsing the display string.
