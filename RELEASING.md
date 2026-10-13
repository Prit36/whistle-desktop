# Releasing Whistle Desktop

The public binary target is Windows x64. Other targets remain source-only until they are built and tested on their own hosts.

1. Update `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md` and `releases/vVERSION.md` together.
2. Review the known limitations and bundled dependency licenses. Update `scripts/runtime-manifest.json` only after verifying the publisher's model/runtime revisions and hashes.
3. Run formatting, Clippy, ordinary tests and native integration tests. Build and inspect the native UI. Exercise a real microphone when hardware is available and document the outcome.
4. Commit and push to `main`. Wait for the Windows CI check to pass.
5. Create an annotated tag matching the package version, for example `git tag -a v0.1.0 -m "Whistle Desktop v0.1.0"`, and push the tag.
6. The release workflow builds the tagged source, reruns checks, prepares the pinned runtime, assembles only approved files, verifies package contents and hashes, and creates a draft GitHub Release with the ZIP and `SHA256SUMS.txt`. It publishes only after all assets are uploaded successfully.
7. Verify the public release, its tag, and the downloaded asset checksum before announcing it.

The public release should describe platform support, unsigned binary status, microphone latency and unverified hardware. Do not represent synthetic transcription fixtures as a recognition-accuracy benchmark.

For a local release build from a clean tagged checkout:

```sh
python scripts/setup_runtime.py
python scripts/package.py --skip-downloads --release
python scripts/verify_package.py dist/Whistle-0.1.0-windows-x64.zip --expect-version v0.1.0
```

`build-info.json` inside the bundle records the source commit, package version, toolchain and runtime identity. `files.sha256.json` covers every packaged file except itself. `SHA256SUMS.txt` covers the downloadable ZIP. Signing is not configured; checksums identify artifacts but do not replace a code-signing certificate.
