# Whistle focused refinement — 14 October 2026

final result: passed

## Source and rendered evidence

The user approved UI 3's layout and asked for restrained visual refinement, clearer state/feedback, keyboard accessibility, and long-transcript behavior. That approved screen is the source, rather than a new concept.

- Source visual truth: [approved UI 3](docs/design/refinement-reference.png).
- Release implementation: [same populated transcript](docs/screenshot.png), [empty editor](docs/design/empty.png), [minimum window](docs/design/compact.png), and [missing-model error](docs/design/setup-error.png).
- Full-view combined comparison: [source and implementation](docs/design/comparison.png).
- Focused combined comparison: [editor spacing and status](docs/design/detail-comparison.png).
- Both source and implementation are 1120 × 760 native pixels, density 1.0, showing the same 21-word transcript, filename, audio duration, and language. No density normalization, scaling, or browser chrome was required.
- Additional native release captures were inspected at 820 × 600 and 1440 × 900. The wide capture is `test-output/polish-wide.png`.

The source and final implementation were viewed together in the composites above, not judged from separate remembered screenshots.

## Findings and comparison history

No actionable P0, P1, or P2 visual findings remain. The first combined debug comparison (`test-output/polish-comparison-first.png`) confirmed the requested refinements without introducing a layout issue. The final release comparison reconfirmed those results after the interaction changes and tests.

Intentional differences from UI 3:

- The full-height sidebar card is replaced by the shared app background and a simple vertical divider. The controls retain their positions and sizes.
- The transcript has 12 px more space beneath its status row, retaining the existing horizontal padding and 17 px text.
- Header controls have explicit **Shortcuts** and **Private · offline** labels, with contextual tooltips. The redundant lock icon is removed.
- Global **Model loaded** is distinct from **Awaiting audio**, **Editable transcript**, **Transcribing**, **Cancelling**, and model/job error states.
- The language control is shorter and uses a quiet input surface and subtler border. Hover/focus retains the mint cue; keyword entry keeps its stronger input boundary.
- A short mint border accent, mint caret, selection treatment, and mint editor focus border supply identity through ordinary interface styling rather than a decorative empty state.

## Required fidelity surfaces

- **Typography:** Existing Inter regular, medium, and semibold families and platform rendering are preserved. Header, section labels, controls, and editor type remain compact and readable. New labels fit the minimum window.
- **Spacing/layout:** The approved header/sidebar/editor composition is unchanged. Source actions, recognition fields, document status, text, and footer stay aligned, with no overlapping or clipped persistent controls in the inspected sizes. Text gains vertical breathing room.
- **Colors/tokens:** Dark neutrals and restrained mint remain. Brighter secondary text is retained; the language field is intentionally lighter in visual weight than the action buttons. Error surfaces remain distinct. Focus uses the existing mint accent.
- **Images/icons:** Existing embedded Phosphor icons remain sharp and consistent. No additional image assets or illustrations were required. Keyboard assistance now has a text label, and privacy is expressed directly in text.
- **Copy/content:** No slogans or redundant helper paragraphs were reintroduced. Processing feedback uses actual elapsed/processed time, not an invented percentage or ETA. Cancellation is stored separately so the source filename remains intact.

## Interaction verification

Seven model-independent tests drive the actual egui widgets and worker-event handling:

1. Typing, copy, clear, and restore.
2. Language selection, microphone dispatch, provisional/confirmed words, stop, and finalization.
3. Minimum-size action/recognition visibility, shortcut popup, file cancellation, and long model diagnostics.
4. Tab and Shift+Tab leaving/returning to the editor, Tab leaving keywords, F6 focus, shortcut help, and Ctrl+Enter recording/stop.
5. Loading, file processing with real event-derived seconds, cancellation, microphone error, retained text, dismiss, and retry.
6. A 1,000-paragraph synthetic transcript: Ctrl+End, editing at the end, undo, full-copy, and visible end-of-document/canvas footer.
7. Live append following at the bottom, respecting a scrolled-back viewport, and preserving the final phrase when its text and completion events arrive together.

Following pauses while the editor has focus. User scrolling back releases the sticky end position. The final-phrase/completion edge case was addressed with a pending-append flag and is covered by a regression assertion.

A real MP3 was also transcribed and captured with the final native release executable. Loading/processing and long-transcript interactions were verified through the real widget harness; those are not presented as physical microphone tests or native long-transcript screenshots.

## Build checks and limits

- Formatting and strict all-target lint passed.
- All 15 model-independent tests passed.
- The separately run native engine integration test passed, for 16 tests total.
- The locked release build passed.
- Existing physical microphone, OS permission, file-dialog automation, and clipboard-delivery limits in `VALIDATION.md` remain. The synthetic long transcript checks editing behavior, not recognition accuracy.

## Implementation checklist

- [x] Approved layout retained; six requested visual details refined.
- [x] Same-state full-view and focused comparisons inspected.
- [x] Standard, compact, error, empty, and wide release captures inspected.
- [x] Keyboard navigation, long editing, processing/cancellation/error recovery, and smart following verified.
- [x] Formatting, lint, tests, native integration test, and release build passed.

No blocking visual or interaction findings remain in the tested scope.
