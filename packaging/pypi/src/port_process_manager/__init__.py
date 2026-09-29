"""Runs the ppm release binary that matches this package's version.

The first run downloads it from GitHub Releases, checks its SHA-256 against the
SHA256SUMS packed into this package at release time, and caches it.
"""

import hashlib
import os
import platform
import subprocess
import sys
import urllib.request
from importlib.metadata import version as package_version

RELEASES = "https://github.com/greenfield-inc/port-process-manager/releases/download"
VERSION = package_version("port-process-manager")
SUMS = os.path.join(os.path.dirname(__file__), "SHA256SUMS")


def fail(message):
    sys.stderr.write("ppm: {}\n".format(message))
    sys.exit(1)


def asset_name():
    arch = {"x86_64": "x86_64", "amd64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}.get(
        platform.machine().lower()
    )
    target = {
        "darwin": "apple-darwin",
        "linux": "unknown-linux-musl",
        "win32": "pc-windows-msvc.exe",
    }.get(sys.platform)
    if not arch or not target:
        fail("no ppm build for {} {}".format(sys.platform, platform.machine()))
    return "ppm-{}-{}".format(arch, target)


def cache_dir():
    home = os.path.expanduser("~")
    if sys.platform == "win32":
        root = os.environ.get("LOCALAPPDATA") or os.path.join(home, "AppData", "Local")
    elif sys.platform == "darwin":
        root = os.path.join(home, "Library", "Caches")
    else:
        root = os.environ.get("XDG_CACHE_HOME") or os.path.join(home, ".cache")
    return os.path.join(root, "port-process-manager", VERSION)


def download(url):
    with urllib.request.urlopen(url, timeout=120) as response:
        return response.read()


def expected_hash(sums, asset):
    for line in sums.splitlines():
        fields = line.split()
        if len(fields) == 2 and fields[1].lstrip("*") == asset:
            return fields[0].lower()
    return None


def install(binary):
    asset = asset_name()
    base = os.environ.get("PPM_DOWNLOAD_URL") or "{}/v{}".format(RELEASES, VERSION)
    with open(SUMS) as sums:
        expected = expected_hash(sums.read(), asset)
    if not expected:
        fail("SHA256SUMS has no entry for {}".format(asset))
    sys.stderr.write("ppm: downloading {} {}\n".format(asset, VERSION))
    try:
        data = download("{}/{}".format(base, asset))
    except OSError as error:
        fail(str(error))
    actual = hashlib.sha256(data).hexdigest()
    if actual != expected:
        fail("checksum mismatch for {}: expected {}, got {}".format(asset, expected, actual))

    os.makedirs(os.path.dirname(binary), exist_ok=True)
    partial = "{}.{}.partial".format(binary, os.getpid())
    with open(partial, "wb") as out:
        out.write(data)
    os.chmod(partial, 0o755)
    os.replace(partial, binary)


def main():
    binary = os.path.join(cache_dir(), "ppm.exe" if sys.platform == "win32" else "ppm")
    if not os.path.exists(binary):
        install(binary)
    args = [binary] + sys.argv[1:]
    if sys.platform == "win32":
        sys.exit(subprocess.call(args))
    os.execv(binary, args)
