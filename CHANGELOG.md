# Changelog

## 0.1.1

First public Windows x64 release.

- Offline Whistle speech recognition through the native Needle runtime.
- MP3, WAV, FLAC, OGG/Vorbis, M4A/AAC and AAC import, including file drops.
- Pause-aware microphone transcription with bounded audio buffers.
- Editable transcript, copy, atomic text export, and undo after clearing.
- Language selection and keyword hints.
- Portable package with the model, runtime, dependency notices, build provenance and SHA-256 checksums.
- Host-filtered dependency notices; packaging can fetch missing dependency sources.
- Manual GitHub CLI releases.

Real microphone hardware remains unverified on the development host. See [validation](VALIDATION.md) for measurements and limitations.

## 0.1.0 — unpublished

Initial packaging attempt required uncached dependencies for other platforms. No binary release was published for this tag.
