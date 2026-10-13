# Contributing

Start with an issue describing the problem or proposed behavior. Keep audio on the user's device and avoid adding network requests to the running application.

Use the pinned Rust toolchain and locked dependencies. Run:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
python scripts/setup_runtime.py
cargo test --locked --test native_engine -- --ignored
```

Changes to audio conversion or segmentation should preserve every input sample and keep buffers bounded. Changes to the C API wrapper must preserve exclusive ownership of the native engine. Keep transcripts and captured audio out of commits and issue attachments unless you intentionally choose to share them.

For microphone changes, describe the device, OS, sample rate and permission behavior you tested. For performance claims, report the hardware, input duration, total processing time and which process the memory measurement covers.
