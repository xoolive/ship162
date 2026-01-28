# Agent development guide

- Code structure:
    - `crates/rs162`: Core library with AIS message decoding (types 1-27), NMEA parsing, and DSP pipeline (demodulation, filters, sample rate adaptation)
    - `crates/ship162`: Full-featured application with TUI, source handling and state management
- Feature flags such as `mqtt`, `pluto`, `rtlsdr`, `soapy` control the supported data sources for the library/application. Unless explicitly asked, always run Cargo commands with `--all-features`.
- Code quality:
    - Always use `--all-targets`.
    - Run `cargo clippy` with `-- -D warnings`.
    - Run `cargo test` with `--doc` and `--workspace` if applicable.
    - Formatting: Run `cargo fmt --all`
- Markdown: use `prettier` for formatting documentation and markdown files. Follow CommonMark specification.
- Decoding specifications and test data:
    - `crates/rs162/data/ais_nmea.txt`: nmea sentences for testing parsers
    - `crates/rs162/data/ais_96k.bin`: iq data for testing demodulators
- Rustdoc: run with `RUSTDOCFLAGS="-D rustdoc::all -A rustdoc::private-doc-tests" cargo doc --all-features --no-deps`

## Code conventions

- Use descriptive variable names (e.g., `mmsi`, `speed_over_ground`, `latitude`)
- Prefer declarative deku attributes for binary decoding
- Document public APIs with `///` doc comments
- Use `tracing` for logging, not `println!`
- Handle errors with `Result<T, E>`, avoid unwrap in library code
- Use `#[must_use]` for important return values

## Release

- Ensure latest commmit on master has no failing CI actions
- `cargo release [patch,minor]`

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
