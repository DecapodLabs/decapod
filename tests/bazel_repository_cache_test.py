"""Opt-in real Bazel download-cache regressions; no external network required.

Run: python3 tests/bazel_repository_cache_test.py
Uses the repository's pinned Bazel through bazelisk; all servers/scratch are owned
by this fixture. This is archive-integrity evidence, not a source-CI receipt.
"""
import hashlib
import http.server
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import unittest

ROOT = Path(__file__).resolve().parents[1]
CACHE_PIN = "0057852bfaa89a56745cba8c7296529d2fc39830"


class RepositoryCacheTest(unittest.TestCase):
    def test_carrier_restores_only_archive_bytes_and_saves_only_accepted_master(self):
        action = (ROOT / ".github/actions/source-ci-profile/action.yml").read_text()
        restore, save = action.split("    - name: Persist accepted Bazel SHA-256 archives\n")
        save = save.split("    - uses: dtolnay/rust-toolchain", 1)[0]
        self.assertIn("uses: actions/cache/restore@" + CACHE_PIN, restore)
        self.assertIn("uses: actions/cache@" + CACHE_PIN, save)
        self.assertIn("github.event_name == 'push' && github.ref == 'refs/heads/master'", save)
        self.assertIn("!(github.event_name == 'push' && github.ref == 'refs/heads/master')", restore)
        self.assertEqual(action.count("path: ~/.cache/bazel/*/cache/repos/v1/content_addressable/sha256"), 2)
        self.assertNotIn("fail-on-cache-miss: true", action)
        self.assertNotIn("lookup-only:", action)
        self.assertNotIn("continue-on-error:", action)

    def test_real_bazel_reuses_verified_archive_during_outage_and_rejects_corruption(self):
        payload = b"Decapod pinned archive fixture\n"
        digest = hashlib.sha256(payload).hexdigest()
        state = {"outage": False, "requests": 0}

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                state["requests"] += 1
                self.send_response(504 if state["outage"] else 200)
                self.end_headers()
                self.wfile.write(b"outage" if state["outage"] else payload)

            def log_message(self, *args):
                pass

        with tempfile.TemporaryDirectory(prefix="decapod-bazel-cache-") as tmp:
            scratch = Path(tmp)
            server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                url = f"http://127.0.0.1:{server.server_port}/payload"
                cache = scratch / "repository-cache"

                def query(name):
                    repo = scratch / name
                    repo.mkdir()
                    (repo / ".bazelversion").write_bytes((ROOT / ".bazelversion").read_bytes())
                    (repo / "MODULE.bazel").write_text(
                        'archive = use_repo_rule("//:fetch.bzl", "archive")\n'
                        f'archive(name="archive", url="{url}", sha256="{digest}")\n'
                    )
                    (repo / "BUILD.bazel").write_text("")
                    (repo / "fetch.bzl").write_text('''def _fetch(ctx):
    ctx.download(url = ctx.attr.url, output = "payload", sha256 = ctx.attr.sha256)
    ctx.file("BUILD.bazel", 'exports_files(["payload"])')
archive = repository_rule(implementation = _fetch, attrs = {
    "url": attr.string(mandatory = True),
    "sha256": attr.string(mandatory = True),
})
''')
                    return subprocess.run(
                        [shutil.which("bazelisk") or "bazelisk", "--batch",
                         f"--output_user_root={scratch / ('output-' + name)}",
                         "--ignore_all_rc_files", "query", "@archive//:payload",
                         f"--repository_cache={cache}", "--repo_contents_cache=",
                         "--http_timeout_scaling=0.1"],
                        cwd=repo, text=True, stdout=subprocess.PIPE,
                        stderr=subprocess.STDOUT, timeout=120,
                        env=dict(os.environ, USE_BAZEL_VERSION=(ROOT / ".bazelversion").read_text().strip()),
                    )

                cold = query("cold")
                self.assertEqual(cold.returncode, 0, cold.stdout)
                entry = cache / "content_addressable/sha256" / digest / "file"
                self.assertEqual(entry.read_bytes(), payload)
                state["outage"] = True
                before = state["requests"]
                warm = query("warm")
                self.assertEqual(warm.returncode, 0, warm.stdout)
                self.assertEqual(state["requests"], before, warm.stdout)
                entry.write_bytes(b"corrupted cache archive")
                corrupt = query("corrupt")
                self.assertNotEqual(corrupt.returncode, 0, corrupt.stdout)
                self.assertGreater(state["requests"], before, corrupt.stdout)
                self.assertIn("504", corrupt.stdout)
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)


if __name__ == "__main__":
    unittest.main(verbosity=2)
