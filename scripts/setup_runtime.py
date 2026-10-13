"""Fetch the pinned native library and weights. Python is setup-only, never a runtime dependency."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import tempfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = json.loads((ROOT / "scripts/runtime-manifest.json").read_text(encoding="utf-8-sig"))


def download(url, expected, destination):
    if destination.exists():
        with destination.open("rb") as cached:
            if hashlib.file_digest(cached, "sha256").hexdigest() == expected:
                print(f"Verified {destination.name}")
                return
    destination.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(dir=destination.parent, suffix=".partial")
    try:
        digest = hashlib.sha256()
        request = urllib.request.Request(url, headers={"User-Agent": "WhistleDesktop-setup/0.1"})
        with os.fdopen(fd, "wb") as target, urllib.request.urlopen(request, timeout=120) as response:
            while block := response.read(1024 * 1024):
                digest.update(block)
                target.write(block)
        if digest.hexdigest() != expected:
            raise RuntimeError(f"SHA-256 mismatch for {destination.name}")
        os.replace(temporary, destination)
        print(f"Downloaded and verified {destination.name}")
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", choices=MANIFEST["wheels"], help="Override the current host's wheel target")
    parser.add_argument("--out", type=Path, default=ROOT / "runtime")
    args = parser.parse_args()
    machine = platform.machine().lower()
    arm = machine in ("aarch64", "arm64")
    tags = {"Windows": "win_arm64" if arm else "win_amd64",
            "Darwin": "macosx_11_0_arm64" if arm else "macosx_11_0_x86_64",
            "Linux": "manylinux2014_aarch64" if arm else "manylinux2014_x86_64"}
    tag = args.tag or tags.get(platform.system())
    if tag not in MANIFEST["wheels"]:
        raise SystemExit("Unsupported host; choose a supported --tag")
    wheel = MANIFEST["wheels"][tag]
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    download(f'https://huggingface.co/Cactus-Compute/whistle/resolve/{MANIFEST["model_revision"]}/whistle.cact',
             MANIFEST["model_sha256"], out / "whistle.cact")
    with tempfile.TemporaryDirectory() as work:
        archive = Path(work) / "runtime.whl"
        download(f'https://huggingface.co/Cactus-Compute/needle3/resolve/{MANIFEST["engine_revision"]}/{wheel["file"]}',
                 wheel["sha256"], archive)
        with zipfile.ZipFile(archive) as source:
            libraries = [name for name in source.namelist()
                         if Path(name).name in ("libneedle3.dll", "libneedle3.so", "libneedle3.dylib")]
            if len(libraries) != 1:
                raise RuntimeError(f"Expected one native library; got {libraries}")
            data = source.read(libraries[0])
            destination = out / Path(libraries[0]).name.replace("needle3", "needle")
            temporary = destination.with_suffix(".partial")
            temporary.write_bytes(data)
            os.replace(temporary, destination)
    for repo, revision, name in [("needle3", MANIFEST["engine_revision"], "NEEDLE-LICENSE.txt"),
                                  ("whistle", MANIFEST["model_revision"], "WHISTLE-LICENSE.txt")]:
        request = urllib.request.Request(f"https://huggingface.co/Cactus-Compute/{repo}/resolve/{revision}/LICENSE")
        with urllib.request.urlopen(request, timeout=60) as response:
            (out / name).write_bytes(response.read())
    (out / "provenance.json").write_text(json.dumps({"tag": tag,
        "library_name": destination.name, "library_sha256": hashlib.sha256(data).hexdigest(),
        **MANIFEST}, indent=2), encoding="utf-8")
    print(f"Ready: {out}. The app can now run offline.")


if __name__ == "__main__":
    main()
