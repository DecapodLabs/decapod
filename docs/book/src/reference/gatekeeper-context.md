# Gatekeeper source context

Gatekeeper checks path policy, protected paths, diff size, credential patterns,
and dangerous source patterns. Context may discharge one exact matched source
span only when the supported parser establishes the relevant meaning. Other
matches on the same line remain independent.

## Explicit scan inputs

Repeat `--paths` for each file:

```text
decapod govern gatekeeper check --paths src/decapod/lib.rs --paths src/decapod/core/dactyl_todo.rs
```

Comma-separated arguments are not expanded. Missing, non-text, or unreadable
explicit files and explicit directories fail rather than reporting a successful
empty scan.
Without `--paths`, staged paths come from Git's NUL-delimited output. A Git
failure is an error. Deleted paths and both sides of renames still participate
in protected-path checks even when there are no remaining bytes to scan.

Literal Rust `include_str!` dependencies are followed, including calls nested
within macro input. General `include!` expansion and `include_bytes!` interpretation
are outside this recognizer. Included files retain credential and path-policy
checks. Included text with an unknown suffix still receives conservative raw
dangerous-pattern checks. Missing, dynamic, unreadable, out-of-repository, or
excessively large dependency graphs cannot establish a successful scan. Moving
source into an included file is not an exemption.

## Supported evidence

- Explicit bearer-scheme syntax remains distinct from ordinary credential
  prose. Short or word-like credentials in explicit syntax remain findings
- Rust password formatting uses exact literal spans and proven runtime
  provenance. Supported immutable environment bindings and a narrowly checked
  random-byte generator can carry evidence through supported local tuple,
  option, branch, and match flow. A helper name alone proves nothing
- Positional SQL parameters require a verified terminal database operation.
  Constructing an operation object alone is insufficient because its SQL can
  escape. Supported local wrappers are resolved through the actual Cargo
  module graph and must forward directly to the external connection contract
- Native shell context distinguishes quoted parameter expansion from
  executable substitutions and unsafe constructs. Dockerfile context keeps
  instruction substitution separate from shell execution in `RUN`
- Fully qualified standard primitive string comparisons consume their input
  as text. They do not make other literals, credentials, or shell operations
  on that source line safe
- Explicit Rust shell or unresolved interpreter construction produces an
  independent review finding, including multiline, alias, and included-input
  forms. A filename's native grammar cannot discharge that execution boundary

These are bounded recognizers, not Rust macro expansion, a compiler, complete
shell interpretation, or dependency-supply-chain verification. Ambiguous
bindings, namespace shadows, unsupported attributes, mutation, unknown
escaping values, unresolved module evidence, and unsupported syntax retain
findings. SQL evidence assumes genuine external database and async-trait
implementations; local path/package substitution and manifest patches cannot
supply that evidence. Cargo configuration, environment overrides, downloaded
code, and toolchains are not authenticated by this source recognizer.
Native data-command recognition likewise assumes standard command
implementations and runtime shell behavior; it does not authenticate `PATH`
or executable contents. Source-defined overrides and known executable options
remain part of the adversarial test boundary.
Arbitrary custom-runner semantics and whole-program reinterpretation of included
text are outside these bounded recognizers. Visible standard-library manifest
substitution or an invalid manifest prevents Rust password-context exemptions;
standalone source without a manifest retains the documented standard-library
assumption.

## Findings that intentionally remain

Literal credentials remain detectable in source, documentation, tests, and
synthetic fixtures. Contextual exemptions do not rely on filenames, comments,
placeholder words, credential length, or test labels. Broader legacy pattern
recognizers retain their documented shape limits. The tests retain short
quoted and explicit-assignment passwords, alongside the old literal setup
instruction, as positive controls.
Typed Rust `let`, `const`, and `static` password/passwd/pwd declarations
are scanned across whitespace and newlines, including short literal values.
The additive textual matcher supports ref/ref mut bindings and simple
array/slice annotations with literal or named lengths. It stops at statement
semicolons, braces, or an earlier equals sign; nested arrays and arbitrary
length expressions are outside this bounded recognizer. It is not a complete
Rust type parser. Detection does
not require successful parsing. Runtime provenance can discharge only its
exact initializer span, so a nearby literal remains a finding. These bounded
recognizers do not supply universal secret detection.

Actual shell command substitutions remain executable-shell findings even
when quoted. For example, a fixed script that queries process identity still
contains a command invocation; recognizing its quoted variables does not
certify every command in the script. Dynamic shell execution, unknown command
provenance, and dangerous operations remain subject to review.

The session setup instruction now identifies the generated password rather
than telling callers to use the separately displayed session token. This is
an instruction correction; it does not add a placeholder exception.

## Verification boundary

Paired tests cover each supported safe form and nearby adversarial changes:
literal fallbacks, mutation and shadowing, operation-string escapes, altered
wrapper implementations, custom macros and methods, shell function overrides,
missing includes, and mixed safe/unsafe lines. Extracted native templates have
byte-equivalence and rendered-output regressions against their original
representations, and remain part of the Cargo package.

Scanner implementation tests do not waive publication policy or establish
container custody. Run the official validator separately and preserve actual
passed, failed, ignored, and unavailable evidence. A new scanner must not act
as its own authority for publication approval.
