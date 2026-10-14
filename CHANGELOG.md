# Changelog

## 0.2.0 — 2026-10-14

- Moved the queue control into the header beside Shortcuts as an icon button, with a waiting-count badge, status tooltip, and accessible label.
- Added folder import with optional subfolders, multiple-file selection, and importing every dropped file/folder or command-line source.
- Added an in-memory sequential queue with per-file status/errors, progress, remove, skip, pause/resume, clear waiting/finished, and retry failed/cancelled files. Files can be added while transcribing or recording.
- Discover audio off the UI thread, sort folder entries, deduplicate overlapping selections, skip folder links, and report unreadable paths and import/queue limits.
- Added batch filename headings, source-based export filenames, and Ctrl+Shift+O for folder import. Cancellation and transcript limits pause remaining work; individual file failures continue to the next item.
- Kept source and recognition controls visible in compact windows and added discovery, queue, drop, recovery, export-name, and command-line regression coverage.

## 0.1.3 — 2026-10-14

- Added offline OGG/Opus voice-note import with a bundled decoder, including .opus files in the file picker.
- Accept Opus voice notes missing the OGG end marker when the final page and packet are complete and the page checksum is valid.
- Added five Opus regression tests covering mono/stereo audio, short clips, cancellation, consumer errors, exact sample counts, and damaged final pages.

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
