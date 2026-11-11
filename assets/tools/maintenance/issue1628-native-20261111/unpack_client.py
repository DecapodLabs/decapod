"""Materialize only the digest-bound reviewed source proposal for native authoring."""
import base64
import gzip
import hashlib
import json
from pathlib import Path
import sys


def main():
    source, manifest_path, destination = map(Path, sys.argv[1:])
    manifest = json.loads(manifest_path.read_text())
    encoded = source.read_bytes()
    assert len(encoded) <= 4 * 1024 * 1024
    assert "sha256:" + hashlib.sha256(encoded).hexdigest() == manifest["material_archive_sha256"]
    data = gzip.decompress(base64.b64decode(encoded, validate=True))
    assert len(data) <= 8 * 1024 * 1024
    files = json.loads(data)
    expected = {x["path"]: x for x in manifest["files"]}
    assert set(files) == set(expected)
    destination.mkdir(mode=0o700, parents=True, exist_ok=False)
    for name, content in files.items():
        assert not name.startswith("/") and ".." not in Path(name).parts and "\0" not in name
        assert not name.startswith((".decapod/", ".git/"))
        raw = content.encode()
        assert len(raw) == expected[name]["bytes"]
        assert "sha256:" + hashlib.sha256(raw).hexdigest() == expected[name]["sha256"]
        target = destination / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(raw)
    print(json.dumps({"reviewed_source_files": len(files), "material_sha256_verified": True}))


if __name__ == "__main__":
    main()
