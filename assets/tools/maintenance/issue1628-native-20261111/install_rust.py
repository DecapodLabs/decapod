"""Prepare exact Rust toolchains from the official rustup distribution; no build."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import urllib.request


def main():
    assert os.environ["CARGO_HOME"] == "/usr/local/cargo"
    assert os.environ["RUSTUP_HOME"] == "/usr/local/rustup"
    root = Path("/root/issue1628-rust-install")
    root.mkdir(mode=0o700, exist_ok=False)
    url = "https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init"
    with urllib.request.urlopen(url + ".sha256", timeout=30) as response:
        published = response.read(2048).decode()
    expected = re.match(r"^([0-9a-f]{64})(?:\s|$)", published)
    assert expected, "Official rustup digest missing"
    with urllib.request.urlopen(url, timeout=120) as response:
        binary = response.read(32 * 1024 * 1024)
        assert not response.read(1), "Official rustup installer exceeded bound"
    assert hashlib.sha256(binary).hexdigest() == expected[1]
    executable = root / "rustup-init"
    executable.write_bytes(binary)
    executable.chmod(0o700)
    subprocess.run([str(executable), "-y", "--no-modify-path", "--profile", "minimal", "--default-toolchain", "1.96.1",
                    "--component", "rustfmt", "--component", "clippy"], check=True, timeout=1200)
    versions = ["1.96.1"]
    if sys.argv[1:] == ["--msrv"]:
        subprocess.run(["/usr/local/cargo/bin/rustup", "toolchain", "install", "1.75.0", "--profile", "minimal",
                        "--component", "rustfmt", "--component", "clippy"], check=True, timeout=1200)
        versions.append("1.75.0")
    for version in versions:
        rustc = Path("/usr/local/rustup/toolchains") / (version + "-x86_64-unknown-linux-gnu/bin/rustc")
        observed = subprocess.check_output([str(rustc), "--version"], text=True).strip()
        assert observed.startswith("rustc " + version + " ")
        print(json.dumps({"toolchain": version, "observed": observed, "installer_sha256": expected[1]}), flush=True)


if __name__ == "__main__":
    main()
