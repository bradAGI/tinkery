# Local voice

## Ownership and consent

Tinkery owns its voice engine, settings and install. It does **not** read Pi Voice,
Pi settings or agent directories and has no Pi fallback.

`voice/setup.mjs` owns discovery/install paths and settings. Engine dependencies
are pinned to `transcribe-cpp@0.2.2` and `@picovoice/pvrecorder-node@1.2.9` in the
Tinkery user-data voice directory. The install records its Node major; a mismatch
asks for reinstall rather than loading incompatible native addons. Node resolution
is `TINKERY_NODE`, then PATH, then the login shell; explicit overrides are authoritative.

Model resolution uses the explicit `TINKERY_VOICE_MODEL` override, optional Tinkery
voice config, or the newest matching Parakeet Q8 GGUF snapshot in the shared
Hugging Face cache. Config defaults are system microphone and English. A missing
engine or model gets one quiet consent line with download size; nothing downloads
silently. Declining suppresses further prompting for that visit/session lifecycle.
The speak key remains discoverable before setup. If Node is missing, pressing it
shows how to point Tinkery to Node. Cloud setup installs **none** of these native
engine/model packages.

## Take lifecycle

The Rust `VoiceHost` boundary keeps process handling out of think. The embedded
setup/helper source runs via Node ESM `--eval`, not a runtime helper file.
Commands/events are versioned JSON lines with take IDs so stale callbacks cannot
modify a newer take. The helper stays goal-blind.

- macOS advertises `⌃⌥Z speak`; elsewhere `F6 speak`. F6 and Ctrl+Alt+Z work
  everywhere. The macOS Ω variant inserts no character and never undoes a skip.
- Only explicit start records. Partial text is muted **at the current cursor**;
  final text inserts at the cursor when it arrives. There is no tracked old anchor.
- Text stays editable and never auto-sends. F2 during a take stops first, waits
  for final (30-second deadline), then sends exactly once.
- Esc cancels/discards the take. Leaving think, stdin EOF and Drop stop/release
  the recorder and terminate the helper. Voice is disabled in review/receipt.
- A recorder read rejected by deliberate stop ends the take; it is not an audio
  failure. Permission, unavailable/busy microphone, install/model/version and
  engine errors get one actionable plain line. A near-zero take clears its preview
  and shows “I didn’t hear anything”. A known silence filler shows “Didn't catch that.”
  Both leave the draft ready to edit or dictate again.

No audio is retained and no transcript is logged. Local voice does not send
anything to a provider; F2 in **live shaping mode** sends the editable text like
any other submitted answer, under that mode's explicit budget.

## Tests, not microphone access

Fake hosts cover keys, inserts, races, finalization, errors and cleanup. Node tests
inject fake engines/recorders and synthetic PCM, with no native addons or network.
PTYs work with `TINKERY_NODE=/tinkery-TEST-no-node` and temporary homes. Never record
real audio to validate a change. Native generated-WAV smoke tests require separate
operator approval and are not in the documented zero-call suite. Preserve the
stop-rejected-read reversion probe.
