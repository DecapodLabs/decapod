"""Version-neutral Cargo inputs for locked Bazel dependency resolution.

The real root manifest remains the source of compiled package metadata. Only its
local package version is immaterial to dependency resolution; every other input
byte is retained so dependency changes still invalidate MODULE.bazel.lock.
"""

def _stable_version(value):
    parts = value.split(".")
    if len(parts) != 3:
        return False
    for part in parts:
        if not part or (len(part) > 1 and part.startswith("0")):
            return False
        for digit in part.elems():
            if digit not in "0123456789":
                return False
    return True

def normalized_cargo_inputs(manifest, lockfile):
    """Return exact-byte normalized inputs, or None for unfamiliar root syntax."""
    for text in [manifest, lockfile]:
        if "\r" in text or '\"\"\"' in text or "'''" in text:
            return None
    lines = manifest.split("\n")
    if len(lines) < 4 or lines[:2] != ["[package]", 'name = "decapod"']:
        return None
    version_index = 3 if lines[2] == 'default-run = "decapod"' else 2
    version_line = lines[version_index]
    if not version_line.startswith('version = "') or not version_line.endswith('"'):
        return None
    version = version_line[len('version = "'):-1]
    if not _stable_version(version):
        return None

    # A comment or quoted value containing the header is never a table.
    packages = ("\n" + lockfile).split("\n[[package]]\n")
    root_index = None
    for index in range(1, len(packages)):
        package = packages[index].split("\n")
        if package[0] != 'name = "decapod"':
            continue
        if root_index != None or len(package) < 2 or package[1] != version_line:
            return None
        for line in package[2:]:
            field = line.lstrip()
            if any([field.startswith(key) for key in ["source", "checksum", "name", "version", "["]]):
                return None
        root_index = index
    if root_index == None:
        return None

    lines[version_index] = 'version = "0.0.0"'
    root = packages[root_index].split("\n")
    root[1] = 'version = "0.0.0"'
    packages[root_index] = "\n".join(root)
    return struct(manifest = "\n".join(lines), lockfile = "\n[[package]]\n".join(packages)[1:])

def _cargo_inputs_impl(ctx):
    inputs = normalized_cargo_inputs(ctx.read(ctx.attr.manifest), ctx.read(ctx.attr.lockfile))
    if inputs == None:
        fail("Cargo inputs must contain one matching canonical local Decapod root version; review the dependency projection before changing their syntax")
    ctx.file("Cargo.toml", inputs.manifest, executable = False)
    ctx.file("Cargo.lock", inputs.lockfile, executable = False)
    ctx.file("BUILD.bazel", 'exports_files(["Cargo.toml", "Cargo.lock"])\n', executable = False)

cargo_inputs = repository_rule(
    implementation = _cargo_inputs_impl,
    attrs = {
        "manifest": attr.label(mandatory = True, allow_single_file = True),
        "lockfile": attr.label(mandatory = True, allow_single_file = True),
    },
)
