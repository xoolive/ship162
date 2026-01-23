# Agent development guide

This guide provides comprehensive instructions for AI agents working on the ship162/rs162 project.

## Project overview

- Real-time maritime AIS decoding and tracking
- End-to-end demodulation from sdr to structured data
- Tui for live monitoring
- Uses [deku](https://github.com/sharksforarms/deku) for declarative binary data decoding

## Project structure

```
ship162/
├── crates/
│   ├── rs162/           # core library (ais decoding, nmea parsing, dsp demodulation)
│   └── ship162/         # live decoding application with tui and source management
```

### Crate responsibilities

- **rs162**: core library with ais message decoding (types 1-27), nmea parsing, and dsp pipeline (demodulation, filters, sample rate adaptation)
- **ship162**: full-featured application with tui, source handling (tcp, mqtt, sdr), and state management

## Setup and build

### Initial build

```sh
cargo build --release --all-features
```

### Building specific components

```sh
# core library only
cargo build -p rs162 --release

# ship162 application
cargo build -p ship162 --release
```

## Testing

### Rust tests

```sh
# Run all tests (workspace-wide)
cargo test --workspace --all-features --all-targets

# Run tests for specific crate
cargo test -p rs162 --all-features

# Run specific test
cargo test test_name -- --nocapture
```

### Benchmarks

```sh
# Run Rust benchmarks
cargo bench
```

## Code quality and style

### Rust

**Linting:**

```sh
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

**Formatting:**

```sh
cargo fmt --all              # Format all code
cargo fmt --all --check      # Check without modifying
```

**Documentation:**

```sh
cargo doc --all-features --no-deps        # Build docs
cargo doc --all-features --no-deps --open # Build and open in browser

# Check for documentation issues
RUSTDOCFLAGS="-D rustdoc::all -A rustdoc::private-doc-tests" cargo doc --all-features --no-deps
```

### Markdown

- Use `prettier` for formatting documentation and markdown files
- Follow CommonMark specification

### Code conventions

- Use descriptive variable names (e.g., `mmsi`, `speed_over_ground`, `latitude`)
- Prefer declarative deku attributes for binary decoding
- Document public APIs with `///` doc comments
- Use `tracing` for logging, not `println!`
- Handle errors with `Result<T, E>`, avoid unwrap in library code
- Use `#[must_use]` for important return values

## Release

- Ensure latest commmit on master has no failing CI actions
- `cargo release [patch,minor]`

## Decoding specifications and test data

### Test samples

- `crates/rs162/data/ais_nmea.txt`: sample nmea sentences for testing parsers
- `crates/rs162/data/ais_96k.bin`: sample iq data for testing demodulators

## Git workflow and commits

### Branching strategy

- `master`: Main development branch (protected)
- Feature branches: `feature/description` or `fix/issue-number`
- Always create PRs for review, never push directly to master

### Commit guidelines

**IMPORTANT:**

- **Never commit without explicit user approval**
- If the user gives you approval for one commit, do not commit again later without explicit user approval.
- Always ask for confirmation before creating commits
- If fixing a GitHub issue, create a dedicated branch and PR

**Conventional Commit message format:**

```
type: brief description (imperative mood)

Optional longer explanation of what changed and why.

Fixes #123
```

### GitHub issues and PRs

**Opening issues:**

```sh
# Never open issues without user acknowledgement
gh issue create --title "Title" --body "Description"
```

**Analyzing issues:**

```sh
# Always read ALL comments before planning
gh issue view 123
gh issue view 123 --comments
```

**Creating pull requests:**

```sh
# After user approves commits
gh pr create --title "Title" --body "Description"

# Link to issue
gh pr create --title "Fix altitude bug" --body "Fixes #123"
```

Update changelog after fixing issues

## Task planning

### Using plan.md

- **Always** use `plan.md` to track complex tasks
- Update frequently as you work through tasks
- **CRITICAL:** Always include a final task item reminding yourself to get user approval before committing
- Structure:

  ```markdown
  ## Current task: [Brief description]

  - [ ] Step 1
  - [ ] Step 2
  - [x] Completed step
  - [ ] ⚠️ STOP: Get explicit user approval before committing

  ## Next:

  - Future tasks
  ```

- Prune completed tasks after commits are merged

### Task breakdown approach

1. **Understand the requirement** - Read issue, analyze code context
2. **Plan steps** - Break into discrete, testable units (always include "Get user approval before committing" as final step)
3. **Execute incrementally** - Small commits, test frequently
4. **Verify** - Run tests, check lints, update docs
5. **Review** - Self-review changes before proposing to user
6. **⚠️ Get explicit user approval** - NEVER commit without asking first

## Support and contributions

- Test thoroughly before proposing changes
- Document breaking changes clearly in PRs
