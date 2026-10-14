# Whistle Desktop implementation plan

## Quality-of-life improvements — 14 October 2026

Implemented. Discovery, queue controls, batch transcripts, export names, and command-line sources are covered by the ordinary test suite. Strict Clippy and native inference passed; native captures verified the compact layout and a mixed-success folder import. The usage guide and changelog describe the behavior and limits.

- Import multiple audio files or a folder, optionally including subfolders. Discover files off the UI thread, sort folders deterministically, filter supported extensions, and deduplicate overlapping imports. Bound discovery and report unreadable paths or limits.
- Queue files on the existing single inference worker. Show each file's status and errors, allow removing waiting files, skipping the current file, pausing/resuming, clearing waiting files, and retrying failures. Cancellation preserves confirmed text and pauses remaining work; an individual decode error does not discard the batch.
- Accept every dropped file/folder and command-line source. Label batch transcript sections with filenames and suggest a source-based export filename. Keep the existing single-file and microphone editing behavior.
- Verify discovery, queue transitions, recovery, text limits, keyboard input, and minimum-window layout with model-independent tests; run formatting, strict Clippy, native integration, a release build, and native render captures.

1. Verify the current Whistle ABI, redistributable runtime, model and Fastframe revisions.
2. Build a native Rust application with Fastframe fonts/text/file helpers and egui/eframe. Use the Fastframe-maintained egui/winit revisions together.
3. Load the native Whistle library once on a dedicated worker. Memory-map the model. Keep every call to the process-global, non-thread-safe C API on that worker.
4. Decode compressed files packet by packet using Symphonia, mix to mono, resample with a persistent anti-aliasing sinc resampler, and feed bounded, pause-aware windows. Never decode the entire file into RAM.
5. Capture microphone samples into a fixed-size, lock-free SPSC ring. Do no allocation, locking, resampling or inference in the audio callback. Decode short phrases once and report overload instead of silently losing speech. The native streaming API was evaluated and remains available through the diagnostic CLI, but is too slow on the tested Windows target to use in the product.
6. Provide file import/drop, live capture, language/keyword controls, cancellation, editable text, copy, clear and atomic text export. Keep audio and transcript ephemeral unless the user exports text.
7. Provide pinned, hash-checked runtime setup and portable packaging. No network client or Python runtime in the application.
8. Validate audio conversion, EOF flushing, cancellation and stream finalization, real Whistle inference, UI rendering, formatting, Clippy and a release build. Record measured performance and remaining platform limits honestly.

Initial target: Windows x64 (this machine). The audio/UI code is portable; runtime setup supports Windows ARM64, macOS ARM64/x64, and Linux x64/ARM64 as well. Other hosts require their own builds and validation.
