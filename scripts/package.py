"""Build and assemble a portable, offline native application with dependency notices."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skip-downloads", action="store_true", help="Reuse the prepared runtime; Cargo may still fetch build dependencies")
    parser.add_argument("--release", action="store_true", help="Require a clean checkout and an exact version tag")
    args = parser.parse_args()
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip())
    if args.release:
        tag = subprocess.check_output(["git", "describe", "--tags", "--exact-match"], cwd=ROOT, text=True).strip()
        if dirty or tag != f"v{version}":
            raise SystemExit("Release requires a clean checkout tagged v" + version)
    if not args.skip_downloads:
        subprocess.run([sys.executable, str(ROOT / "scripts/setup_runtime.py")], cwd=ROOT, check=True)
    subprocess.run(["cargo", "build", "--release", "--locked", "--bins"], cwd=ROOT, check=True)
    out = ROOT / "dist/Whistle"
    out.mkdir(parents=True, exist_ok=True)
    included = set()

    def copy(source, destination):
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
        included.add(destination.relative_to(out).as_posix())

    def write(destination, value):
        destination.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
        included.add(destination.relative_to(out).as_posix())
    suffix = ".exe" if platform.system() == "Windows" else ""
    copy(ROOT / f"target/release/whistle-desktop{suffix}", out / f"Whistle{suffix}")
    copy(ROOT / f"target/release/whistle-cli{suffix}", out / f"whistle-cli{suffix}")
    for name in ["README.md", "LICENSE", "VALIDATION.md", "CHANGELOG.md", "RELEASING.md", "CONTRIBUTING.md", "docs/screenshot.png"]:
        if (ROOT / name).exists():
            copy(ROOT / name, out / name)
    runtime = ROOT / "runtime"
    library = "libneedle.dll" if suffix else ("libneedle.dylib" if platform.system() == "Darwin" else "libneedle.so")
    if not (runtime / "whistle.cact").is_file() or not (runtime / library).is_file():
        raise SystemExit("Runtime is incomplete. Run scripts/setup_runtime.py first.")
    provenance = json.loads((runtime / "provenance.json").read_text(encoding="utf-8"))
    for name, expected in [("whistle.cact", provenance["model_sha256"]), (library, provenance["library_sha256"])]:
        with (runtime / name).open("rb") as source:
            if hashlib.file_digest(source, "sha256").hexdigest() != expected:
                raise SystemExit(f"Runtime checksum mismatch: {name}. Rerun setup_runtime.py.")
    for name in ["whistle.cact", library, "provenance.json", "NEEDLE-LICENSE.txt", "WHISTLE-LICENSE.txt"]:
        copy(runtime / name, out / "runtime" / name)
    compiler = subprocess.check_output(["rustc", "-vV"], text=True)
    host = next(line.removeprefix("host: ") for line in compiler.splitlines() if line.startswith("host: "))
    # A build only fills the cache for its host. Metadata must not demand an
    # offline cache for every other OS; narrow the graph and allow Cargo fetches.
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--filter-platform", host,
        "--format-version", "1"], cwd=ROOT))
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    included_packages = set()
    pending = [metadata["resolve"]["root"]]
    while pending:
        package_id = pending.pop()
        if package_id not in included_packages:
            included_packages.add(package_id)
            pending.extend(nodes[package_id]["dependencies"])
    notices = out / "third-party"
    notices.mkdir(exist_ok=True)
    copy(ROOT / "assets/Phosphor-LICENSE.txt", notices / "Phosphor-LICENSE.txt")
    index = []
    for package in metadata["packages"]:
        if package["id"] not in included_packages:
            continue
        source = Path(package["manifest_path"]).parent
        if source == ROOT:
            continue
        target = notices / f'{package["name"]}-{package["version"]}'
        license_roots = [source] + [p for p in list(source.parents)[:2] if (p / "Cargo.toml").is_file()]
        files = [p for root in license_roots for p in root.iterdir()
                 if p.is_file() and p.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT"))]
        for subdir in [source / "licenses", source / "fonts"]:
            if subdir.is_dir():
                files += [p for p in subdir.rglob("*") if p.is_file() and
                          (p.suffix.lower() in (".txt", ".md") or "licen" in p.name.lower())]
        if files:
            target.mkdir(exist_ok=True)
            for license_file in files:
                destination = target / license_file.name
                if destination.exists() and destination.read_bytes() != license_file.read_bytes():
                    destination = target / f"{license_file.parent.name}-{license_file.name}"
                copy(license_file, destination)
        index.append({key: package.get(key) for key in ["name", "version", "license", "repository"]})
    write(notices / "dependencies.json", index)
    system = {"Windows": "windows", "Darwin": "macos", "Linux": "linux"}[platform.system()]
    arch = "arm64" if platform.machine().lower() in ("arm64", "aarch64") else "x64"
    write(out / "build-info.json", {"version": version, "source_commit": commit, "source_dirty": dirty,
        "platform": f"{system}-{arch}", "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
        "model_sha256": provenance["model_sha256"], "library_sha256": provenance["library_sha256"]})
    checksums = {}
    for name in sorted(included):
        with (out / name).open("rb") as source:
            checksums[name] = hashlib.file_digest(source, "sha256").hexdigest()
    write(out / "files.sha256.json", checksums)
    archive = ROOT / f"dist/Whistle-{version}-{system}-{arch}.zip"
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6, strict_timestamps=False) as output:
        # Only files explicitly assembled in this run are distributable. Never
        # sweep an existing output/runtime directory into a public release.
        for name in sorted(included):
            output.write(out / name, "Whistle/" + name)
    with archive.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    (ROOT / "dist/SHA256SUMS.txt").write_text(f"{digest}  {archive.name}\n", encoding="utf-8")
    print(f"Ready: {out / ('Whistle' + suffix)}\nBundle: {archive}")


if __name__ == "__main__":
    main()
