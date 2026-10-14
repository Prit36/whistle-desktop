# Changelog

## 0.1.2 — 2026-10-14

- Refined the original dark interface with restrained mint accents, polished typography, brighter secondary text, and clearer input boundaries.
- Removed promotional headings, breadcrumbs, decorative empty states, redundant instructions, and the bottom shortcut strip to prioritize the transcript.
- Added icons, shortcut hints beside source actions, a keyboard reference menu, and compact model/job feedback with a recording timer.
- Kept all recognition fields and core actions visible at 820 × 600, with adaptive recording and cancellation labels.
- Removed the sidebar card, softened the language control, added editor breathing room, and gave the editor a mint focus border and caret.
- Clarified privacy, shortcut, model, and transcript labels; processing shows elapsed or processed time, and cancellation retains the original filename.
- Added smart following of live text, Tab/Shift+Tab navigation, and F6 to focus the transcript.
- Added seven model-independent UI tests, including a 1,000-paragraph transcript, keyboard navigation, processing/error recovery, and scroll-follow behavior.
- Bundled Phosphor icons locally and included their license in portable packages.

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
