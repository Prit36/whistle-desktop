"""Verify a portable ZIP without extracting or executing it."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import zipfile


def verify(path, expected_version=None, expected_commit=None):
    with zipfile.ZipFile(path) as archive:
        entries = archive.infolist()
        names = [entry.filename for entry in entries]
        if len(names) != len(set(names)) or sum(entry.file_size for entry in entries) > 512 * 1024 * 1024:
            raise ValueError("Duplicate entries or excessive uncompressed size")
        for name in names:
            parts = PurePosixPath(name).parts
            if not parts or parts[0] != "Whistle" or ".." in parts or "\\" in name or ":" in name:
                raise ValueError(f"Unsafe archive path: {name}")
        if archive.testzip() is not None:
            raise ValueError("ZIP CRC verification failed")
        manifest = json.loads(archive.read("Whistle/files.sha256.json"))
        expected_names = {"Whistle/" + name for name in manifest} | {"Whistle/files.sha256.json"}
        if set(names) != expected_names:
            raise ValueError("Archive contains missing or unlisted files")
        for name, expected in manifest.items():
            if hashlib.sha256(archive.read("Whistle/" + name)).hexdigest() != expected:
                raise ValueError(f"File checksum mismatch: {name}")
        info = json.loads(archive.read("Whistle/build-info.json"))
        if expected_version and info["version"] != expected_version.removeprefix("v"):
            raise ValueError("Package version differs from release tag")
        if expected_commit and info["source_commit"] != expected_commit:
            raise ValueError("Package source commit differs from requested commit")
        if info["source_dirty"]:
            raise ValueError("Package was built from uncommitted changes")
        for name, key in [("runtime/whistle.cact", "model_sha256"), ("runtime/libneedle.dll", "library_sha256")]:
            if info["platform"] == "windows-x64" and manifest[name] != info[key]:
                raise ValueError(f"Runtime identity mismatch: {name}")
        for name in ["LICENSE", "README.md", "runtime/NEEDLE-LICENSE.txt", "runtime/WHISTLE-LICENSE.txt", "third-party/dependencies.json"]:
            if name not in manifest:
                raise ValueError(f"Missing distribution notice: {name}")
    sums = path.parent / "SHA256SUMS.txt"
    with path.open("rb") as source:
        expected_line = f"{hashlib.file_digest(source, 'sha256').hexdigest()}  {path.name}"
    if expected_line not in sums.read_text(encoding="utf-8").splitlines():
        raise ValueError("ZIP differs from SHA256SUMS.txt")
    return info


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--expect-version")
    parser.add_argument("--expect-commit")
    args = parser.parse_args()
    print(json.dumps(verify(args.archive, args.expect_version, args.expect_commit), indent=2))


if __name__ == "__main__":
    main()
