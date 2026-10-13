# Validation — 13 October 2026

Test host: Windows x64, Intel Core i5-12400F (6 cores / 12 logical processors), Rust 1.98.1. Release profile uses thin LTO, one codegen unit and stripped executables. No GPU inference.

## Measured inference

The fixture is a locally generated English Windows speech-synthesis recording, encoded as 44.1 kHz stereo MP3. The longer fixture repeats the recording four times, spanning multiple inference windows. Language was forced to English. Measurements include model loading, decoding, downmixing, resampling, inference and transcript assembly; OS process-launch overhead is reported separately by the benchmark script.

| Input / pipeline | Audio duration | In-process time | Speed vs realtime | Peak CLI working set |
| --- | ---: | ---: | ---: | ---: |
| MP3 / file phrases | 9.093 s | 0.418 s | 21.7× | 57.1 MiB |
| MP3 / file phrases, multiple windows | 36.522 s | 1.059 s | 34.5× | 56.9 MiB |
| MP3 / microphone phrase pipeline, simulated input | 9.093 s | 0.246 s | 36.9× | 56.2 MiB |

Model load was approximately 18 ms. The short fixture's transcript matched its spoken text. The long fixture produced exactly four repetitions after the final segmentation change. These are small synthetic fixtures, not an accuracy evaluation or a universal performance guarantee. The memory figures cover the native inference CLI; the desktop adds fonts, text editing, a window and graphics-driver memory. Microphone simulation is unpaced file input and excludes hardware capture latency.

Reproduce with `python scripts/benchmark.py AUDIO.mp3` or add `--live`. The script reads peak working-set counters from the child process on Windows. Fixture-generation tools (Windows speech synthesis and FFmpeg) are only for validation, never application dependencies.

## Checks completed

- Eight model-independent tests: exact EOF sample counts at six sample rates, packet-boundary independence, anti-alias filtering, stereo downmixing, invalid input/cancellation, language/keyword validation, lossless segment boundaries and pause latency.
- Native model integration test: silence suppression, stream-tail finalization/reset, single-instance ownership, oversized-window rejection, worker recovery after a missing file and cancellation before decoding.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets`, explicit native integration test and locked release build.
- Native OpenGL window rendered and captured with an empty transcript and a real imported MP3 transcript. Model setup, file worker, UI delivery and editor rendering exercised end to end.
- Real WAV (22.05 kHz mono) and FLAC, OGG/Vorbis, M4A/AAC and AAC (48 kHz stereo) imports decoded and transcribed the fixture successfully.
- The application links directly to the publisher's native library. The Python wrapper and its network/download-counting/telemetry behavior are not included. Model and wheel hashes are pinned and checked during setup.

## Material limits

- No microphone is connected on this host. CPAL correctly reports the missing device; real hardware capture, device removal and OS permission interactions still need testing on a machine with an input device.
- Native Whistle streaming took 17.8 s for the 9.1 s fixture in an earlier test. The shipped app instead uses efficient pause-aware phrases, committing after approximately 500 ms of quiet, or within a six-second audio window plus inference time. The native streaming path is retained only as a CLI diagnostic.
- Word timestamps were not reliable enough for cutting the repeated long fixture without duplicated text. The shipped implementation cuts using audio energy and does not overlap/redecode audio. A forced boundary during continuous speech can affect a word's recognition.
- No automated recognition-accuracy claims are made for noise, accents, music or the other six languages. The model supports English, German, French, Spanish, Italian, Dutch and Polish.
- Cancellation waits for the current native call. Audio remains bounded as recordings grow; accumulated text, editor undo history and copy/export snapshots use additional memory.
- Windows x64 was built and tested. Other platforms have setup support but require their own native build and validation.

The Windows portable bundle includes the pinned model and native runtime, and runs without a Python installation or network connection.
