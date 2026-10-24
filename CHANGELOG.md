# Changelog

## Unreleased

### Fixed

- Reject zero and out-of-range sender MMSIs in CRC-valid IQ frames, and check numeric range before MMSI country lookup.
- Decode valid AIS messages shorter than 14 bytes, preserving leading-zero coast-station identities; read optional type-20 slot fields in big-endian order.

- Upgrade Desperado and its SDR drivers to 0.5.1.
- Fix HackRF-only builds and skip RTL-SDR examples when their feature is disabled.
- Replace the SSH recursion macro with an explicitly boxed recursive call, avoiding Clippy 1.99's generated `double_must_use` error.
- Stop and join IQ demodulators on shutdown, including idle readers and headless signals.
- Propagate IQ read errors and exit after all finite sources end. Return failure for missing or truncated recordings.
- Observe source-task failures and abort timed-out tasks instead of silently detaching them.
- Reject invalid IQ sample rates and Airspy serials before starting readers.
- Restore the terminal when finite sources finish or the UI fails, and keep Redis retries cancellable on quit.
- Ignore the expected `vessels-v2/status` MQTT service topic without hiding invalid AIS payloads or unknown-topic errors.

### Added

- Shared IQ supervision: reopen live SDR sources after failures with a five-second retry delay, stopping the previous reader first.
- Colored source-status summary and an `s`-toggle panel with disconnect/error details. Render snapshots without holding the shared state lock during terminal I/O.
- Register all configured sources before startup and track MQTT connection/reconnection callbacks even without incoming AIS data.
- Match Jet1090's source icons and Tailwind colors, including zero counts, centered shortcut bar and context-sensitive error-window controls. Support upper/lowercase navigation and top-of-list shortcuts.
- An `e` warnings/errors window, unread-error badge, bounded/coalesced diagnostic history, and a stderr report after restoring the terminal on quit.
- CI checks for isolated feature builds, plus hardware-free supervision, finite-file CLI, MQTT callback, and TUI regression tests.
