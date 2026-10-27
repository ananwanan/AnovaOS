# SPDX-License-Identifier: Apache-2.0
# Acquire locked compiler inputs with transport fallback, verified caches and atomic publication.
"""An invalid digest is fatal; only transport failures permit trying another locked URL."""
import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile
from urllib.parse import urlsplit


def verify(path, expected):
    path = Path(path)
    if path.is_symlink() or not path.is_file():
        raise RuntimeError("Expected a regular source archive: " + str(path))
    with path.open("rb") as stream:
        actual = hashlib.file_digest(stream, "sha256").hexdigest()
    if actual != expected:
        raise RuntimeError("Input checksum mismatch: " + path.name)


def fetch(path, urls, expected, cache=None):
    path = Path(path)
    if path.exists() or path.is_symlink():
        verify(path, expected)
        return
    if not urls or len(expected) != 64 or any(c not in "0123456789abcdef" for c in expected):
        raise RuntimeError("Invalid locked source definition")
    for url in urls:
        parsed = urlsplit(url)
        if parsed.scheme != "https" or not parsed.hostname or parsed.username or parsed.password:
            raise RuntimeError("Source URLs must use credential-free HTTPS")
    path.parent.mkdir(parents=True, exist_ok=True)
    # A unique same-directory partial cannot replace a stale file, symlink or previous good archive.
    with tempfile.TemporaryDirectory(prefix="fetch-", dir=path.parent) as work:
        candidate = Path(work) / "archive"
        cached = Path(cache) / path.name if cache else None
        if cached is not None and (cached.exists() or cached.is_symlink()):
            verify(cached, expected)
            shutil.copyfile(cached, candidate)
            verify(candidate, expected)
        else:
            for url in urls:
                candidate.unlink(missing_ok=True)
                try:
                    subprocess.run([
                        "curl", "--fail", "--location", "--proto", "=https",
                        "--proto-redir", "=https", "--tlsv1.2", "--connect-timeout", "15",
                        "--retry", "1", "--max-time", "180", "-o", str(candidate), url,
                    ], check=True)
                except subprocess.CalledProcessError:
                    continue
                verify(candidate, expected)
                break
            else:
                raise RuntimeError("All locked source transports failed: " + path.name)
        candidate.replace(path)
