"""Exercise the real projection in Starlark, then verify its committed lock binding."""

load(":cargo_inputs.bzl", "normalized_cargo_inputs")

_MANIFEST = '[package]\nname = "decapod"\ndefault-run = "decapod"\nversion = "{version}"\n[dependencies]\nserde = "1.0"\n'
_LOCK = 'version = 4\n\n[[package]]\nname = "decapod"\nversion = "{version}"\ndependencies = [\n "serde",\n]\n\n[[package]]\nname = "serde"\nversion = "1.0.228"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\nchecksum = "original"\n'

def _require(condition, message):
    if not condition:
        fail(message)

def _projection_cases():
    baseline = normalized_cargo_inputs(_MANIFEST.format(version = "0.107.5"), _LOCK.format(version = "0.107.5"))
    _require(baseline != None, "canonical inputs must be accepted")
    for version in ["0.108.0", "0.108.1", "1.0.0"]:
        release = normalized_cargo_inputs(_MANIFEST.format(version = version), _LOCK.format(version = version))
        _require(release == baseline, "version-only release changed dependency inputs")
    for before, after in [('serde = "1.0"', 'serde = "2.0"'), ('serde = "1.0"', 'serde = { version = "1.0", features = ["derive"] }')]:
        changed = normalized_cargo_inputs(_MANIFEST.format(version = "0.108.0").replace(before, after), _LOCK.format(version = "0.108.0"))
        _require(changed != None and changed.manifest != baseline.manifest, "dependency/feature drift was hidden")
    for before, after in [('version = "1.0.228"', 'version = "1.0.229"'), ('checksum = "original"', 'checksum = "changed"'), ("registry+https://github.com/rust-lang/crates.io-index", "registry+https://example.invalid/index")]:
        changed = normalized_cargo_inputs(_MANIFEST.format(version = "0.108.0"), _LOCK.format(version = "0.108.0").replace(before, after))
        _require(changed != None and changed.lockfile != baseline.lockfile, "locked dependency drift was hidden")
    manifest = _MANIFEST.format(version = "0.108.0")
    lockfile = _LOCK.format(version = "0.108.0")
    for invalid in [
        _LOCK.format(version = "0.107.5"),
        lockfile + '\n[[package]]\nname = "decapod"\nversion = "0.108.0"\n',
        lockfile.replace('name = "decapod"\nversion = "0.108.0"', 'name = "decapod"\nversion = "0.108.0"\nsource = "registry+https://example.invalid/index"'),
        lockfile.replace('name = "decapod"\nversion = "0.108.0"', 'name = "decapod"\nversion = "0.108.0"\nchecksum = "unexpected"'),
        lockfile.replace('name = "decapod"', 'name = "other"'),
        lockfile.replace('[[package]]\nname = "decapod"', '# [[package]]\nname = "decapod"'),
        lockfile + '\nnote = """unfamiliar"""\n',
    ]:
        _require(normalized_cargo_inputs(manifest, invalid) == None, "ambiguous/mismatched root lock must fail")
    for invalid in [manifest.replace("[package]", "[other]"), manifest.replace('version = "0.108.0"', 'version = "00.108.0"'), manifest.replace("\n", "\r\n"), manifest + "\nnote = '''unfamiliar'''\n"]:
        _require(normalized_cargo_inputs(invalid, lockfile) == None, "unfamiliar manifest must fail")
    _require(baseline.manifest == _MANIFEST.format(version = "0.0.0"), "non-version manifest bytes changed")
    _require(baseline.lockfile == _LOCK.format(version = "0.0.0"), "non-version lock bytes changed")

def _cargo_inputs_test_impl(ctx):
    _projection_cases()
    files = [ctx.file.script, ctx.file.manifest, ctx.file.lockfile, ctx.file.module_lock]
    executable = ctx.actions.declare_file(ctx.label.name + ".sh")
    paths = ['"$TEST_SRCDIR/{}/{}"'.format(ctx.workspace_name, file.short_path) for file in files]
    ctx.actions.write(executable, "#!/bin/sh\nset -eu\nexec python3 " + " ".join(paths) + "\n", is_executable = True)
    return [DefaultInfo(executable = executable, runfiles = ctx.runfiles(files = files))]

cargo_inputs_test = rule(
    implementation = _cargo_inputs_test_impl,
    test = True,
    attrs = {
        "script": attr.label(allow_single_file = True, default = "//.config/build:cargo_inputs_policy.py"),
        "manifest": attr.label(allow_single_file = True, default = "@decapod_cargo_inputs//:Cargo.toml"),
        "lockfile": attr.label(allow_single_file = True, default = "@decapod_cargo_inputs//:Cargo.lock"),
        "module_lock": attr.label(allow_single_file = True, default = "//:MODULE.bazel.lock"),
    },
)
