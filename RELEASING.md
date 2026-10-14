# Releasing Whistle Desktop

The public binary target is Windows x64. Other targets remain source-only until they are built and tested on their own hosts.

1. Update `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md` and `releases/vVERSION.md` together.
2. Review the known limitations and bundled dependency licenses. Update `scripts/runtime-manifest.json` only after verifying the publisher's model/runtime revisions and hashes.
3. Run formatting, Clippy, ordinary tests and native integration tests. Build and inspect the native UI. Exercise a real microphone when hardware is available and document the outcome.
4. Run the local checks below, then commit and push to `main`.
5. Create an annotated tag matching the package version. Build and verify the package locally from that clean tagged checkout, then push the tag.
6. Use GitHub CLI to create a draft release with the ZIP and `SHA256SUMS.txt`. Publish the draft after both assets are uploaded successfully.
7. Download the public assets and verify their checksums and source commit before announcing the release.

Every release is built, checked and published manually using GitHub CLI.

The public release should describe platform support, unsigned binary status, microphone latency and unverified hardware. Do not represent synthetic transcription fixtures as a recognition-accuracy benchmark.

Local checks before committing:

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
python scripts/setup_runtime.py
cargo test --locked --test native_engine -- --ignored
```

After committing the version changes, publish manually (adjust the version each time):

```powershell
$version = "0.1.2"
$tag = "v$version"
$repo = "Prit36/whistle-desktop"
git push --no-follow-tags origin main
git tag -a $tag -m "Whistle Desktop $tag"
python scripts/package.py --skip-downloads --release
$archive = "dist/Whistle-$version-windows-x64.zip"
python scripts/verify_package.py $archive --expect-version $tag --expect-commit (git rev-parse HEAD)
git push origin $tag
gh release create $tag $archive dist/SHA256SUMS.txt --repo $repo --draft --verify-tag --title "Whistle Desktop $tag" --notes-file "releases/$tag.md"
gh release edit $tag --repo $repo --draft=false --latest
```

Stop if any command fails. Do not move an existing public tag or replace a published release's assets; increment the version instead. If an upload fails, keep the release in draft while you repair it.

`build-info.json` inside the bundle records the source commit, package version, toolchain and runtime identity. `files.sha256.json` covers every packaged file except itself. `SHA256SUMS.txt` covers the downloadable ZIP. Signing is not configured; checksums identify artifacts but do not replace a code-signing certificate.
