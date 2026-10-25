# Changelog

## [0.2.1] - 2026-10-25

### Changed

- Deserialize source configuration through typed, Serde-derived raw inputs and `TryFrom` validation, preserving supported formats and strict field checks while improving diagnostics.

### Fixed

- Avoid repeating the underlying SoapySDR/driver error in the sources panel.
- Accept nested RTL-SDR sample rates and structured SoapySDR `{ args, sample_rate }` tables (with their respective features), alongside canonical source-level settings; reject conflicting rates explicitly.
- Preserve source and field details in TOML configuration errors, and name disabled backend features instead of reporting a generic untagged-enum failure.
- Add configuration-loading regressions for mixed TCP/MQTT/WebSocket/RTL inputs, structured SoapySDR settings, and builds without optional backend support.

## [0.2.0] - 2026-10-24

Changes since [v0.1.5](https://github.com/xoolive/ship162/releases/tag/v0.1.5).

### Added

- Supervise live RTL-SDR, Airspy, HackRF and Soapy IQ sources: reconnect after failures with a five-second retry delay, stopping and joining the previous reader first.
- Report source connection, reconnection, failure, completion and shutdown states. Register every configured source before rendering, including quiet MQTT receivers.
- Add Jet1090-style TUI source indicators (`● ○ ↻ ✕`), a source-details panel (`S`), centered contextual controls and top-of-list navigation.
- Add a warnings/errors window (`E`), unread-error badge, bounded diagnostic history and grouping of repeated events. Retained diagnostics are printed to stderr after restoring the terminal on quit.
- Configure fixed-delay reconnect policies for TCP and WebSocket sources through TOML `retry` settings.
- Track MQTT connection and reconnection callbacks independently of incoming AIS messages.
- Add CI checks for isolated feature configurations and regressions covering source shutdown, finite-file CLI behavior, MQTT lifecycle, AIS validation and TUI rendering/navigation.

### Fixed

- Reject zero and out-of-range sender MMSIs in CRC-valid IQ frames. Check numeric bounds before MMSI metadata lookup without filtering on country tables or coordinates.
- Decode valid AIS messages shorter than 14 bytes, including type-20 coast-station reports with leading-zero identities.
- Read optional type-20 slot-reservation fields in big-endian order.
- Stop and join IQ demodulators on shutdown, including idle readers and headless SIGINT/SIGTERM/SIGHUP handling.
- Propagate IQ read errors, terminate after all finite sources finish, and return failure for missing or truncated recordings.
- Observe source-task failures and abort timed-out tasks instead of silently detaching them.
- Reject invalid IQ sample rates and Airspy serials before starting readers.
- Restore the terminal on finite-source completion and UI failure; allow quit to interrupt Redis retry waits.
- Keep diagnostics separate from JSON output and TUI stdout. Reject conflicting interactive/verbose/stdout-log combinations and report config/log-file failures with context.
- Ignore the expected `vessels-v2/status` MQTT service topic without suppressing malformed AIS payloads or unknown-topic errors.
- Fix HackRF-only builds and skip RTL-SDR examples when that feature is disabled.
- Replace the SSH recursion macro with an explicitly boxed recursive call, avoiding Clippy 1.99's generated `double_must_use` error.

### Changed

- Upgrade Desperado and its RTL-SDR/Airspy/HackRF drivers from the 0.4 series to 0.5.1.
- Use protocol-specific source configuration types, clearer parsing errors and explicit disabled-feature diagnostics. Unknown TOML source fields are rejected rather than silently ignored.
- Render TUI snapshots without holding the shared state lock during terminal I/O.
- Refresh dependencies, including dirs 7, tokio-tungstenite 0.30, ratatui, Redis, Tokio, Clap, serde and thiserror; update GitHub Actions dependencies.
- Update the transitive spin dependency to 0.9.9, replacing the yanked 0.9.8 package.

### Compatibility and known limitations

- `--interactive` and `--verbose` cannot be combined. `--log-file -` cannot be combined with either mode because stdout belongs to the TUI or JSON stream.
- Source TOML is validated more strictly. Use `retry = { strategy = "fixed", delay_seconds = 5 }` for configurable TCP/WebSocket retry delays.
- IQ file input remains raw CU8 at 288 kHz. Configurable capture metadata/format and WAV-IQ integration are separate follow-ups.
- MQTT remains Digitraffic-specific: the legacy `mqtt` string is passed as a client ID, not used to select a custom broker/topic ([#65](https://github.com/xoolive/ship162/issues/65)). MQTT still requires a source build with `--features mqtt`; prebuilt binaries do not include it.
- Some SDR URI selectors/query settings remain incomplete; prefer explicit TOML device/settings configuration until the shared-source migration.
- Builds and file/mock-based tests cover the supported targets, but physical USB unplug/replug and hardware current-draw checks were not performed for these changes.

## v0.1.5 — 2026-06-12

Previous published release. Earlier release notes are available in the [GitHub release history](https://github.com/xoolive/ship162/releases).

[0.2.0]: https://github.com/xoolive/ship162/compare/v0.1.5...v0.2.0
