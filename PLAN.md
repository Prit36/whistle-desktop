# Whistle Desktop implementation plan

1. Verify the current Whistle ABI, redistributable runtime, model and Fastframe revisions.
2. Build a native Rust application with Fastframe fonts/text/file helpers and egui/eframe. Use the Fastframe-maintained egui/winit revisions together.
3. Load the native Whistle library once on a dedicated worker. Memory-map the model. Keep every call to the process-global, non-thread-safe C API on that worker.
4. Decode compressed files packet by packet using Symphonia, mix to mono, resample with a persistent anti-aliasing sinc resampler, and feed bounded, pause-aware windows. Never decode the entire file into RAM.
5. Capture microphone samples into a fixed-size, lock-free SPSC ring. Do no allocation, locking, resampling or inference in the audio callback. Decode short phrases once and report overload instead of silently losing speech. The native streaming API was evaluated and remains available through the diagnostic CLI, but is too slow on the tested Windows target to use in the product.
6. Provide file import/drop, live capture, language/keyword controls, cancellation, editable text, copy, clear and atomic text export. Keep audio and transcript ephemeral unless the user exports text.
7. Provide pinned, hash-checked runtime setup and portable packaging. No network client or Python runtime in the application.
8. Validate audio conversion, EOF flushing, cancellation and stream finalization, real Whistle inference, UI rendering, formatting, Clippy and a release build. Record measured performance and remaining platform limits honestly.

Initial target: Windows x64 (this machine). The audio/UI code is portable; runtime setup supports Windows ARM64, macOS ARM64/x64, and Linux x64/ARM64 as well. Other hosts require their own builds and validation.
