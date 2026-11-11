"""Install only the digest-verified official Decapod v0.138.2 release."""
import hashlib
import json
from pathlib import Path
import sys
import tarfile
import urllib.request

RELEASE = "v0.138.2"
NAME = "decapod-x86_64-unknown-linux-gnu.tar.xz"
SIZE = 8241216
ARCHIVE = "718fa969ca3eb7372a7b31e72e8572e3517ac61f969adad03790928a9d03dc06"
BINARY = "31bd8e3983dd328a1796da8af827647be691982c91d7ec7a7b11c7fdc2b4d723"


def main():
    directory = Path(sys.argv[1]).resolve()
    directory.mkdir(mode=0o700, parents=True, exist_ok=True)
    url = "https://api.github.com/repos/DecapodLabs/decapod/releases/tags/" + RELEASE
    request = urllib.request.Request(url, headers={"Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28"})
    with urllib.request.urlopen(request, timeout=30) as response:
        release = json.load(response)
    assets = [a for a in release["assets"] if a["name"] == NAME]
    assert release["id"] == 442229037 and release["tag_name"] == RELEASE and not release["draft"]
    assert len(assets) == 1
    asset = assets[0]
    download = "https://github.com/DecapodLabs/decapod/releases/download/" + RELEASE + "/" + NAME
    assert asset["id"] == 737052855 and asset["size"] == SIZE and asset["state"] == "uploaded"
    assert asset["digest"] == "sha256:" + ARCHIVE and asset["browser_download_url"] == download
    archive = directory / NAME
    with urllib.request.urlopen(download, timeout=60) as response, archive.open("wb") as handle:
        while block := response.read(1024 * 1024):
            handle.write(block)
            assert handle.tell() <= SIZE
    assert archive.stat().st_size == SIZE and hashlib.sha256(archive.read_bytes()).hexdigest() == ARCHIVE
    with tarfile.open(archive) as tar:
        members = [m for m in tar.getmembers() if m.isfile() and Path(m.name).name == "decapod"]
        assert len(members) == 1
        data = tar.extractfile(members[0]).read()
    assert hashlib.sha256(data).hexdigest() == BINARY
    controller = directory / "decapod"
    controller.write_bytes(data)
    controller.chmod(0o700)
    print(json.dumps({"release": RELEASE, "binary_sha256": BINARY, "archive_sha256": ARCHIVE}))


if __name__ == "__main__":
    main()
