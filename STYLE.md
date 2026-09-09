# Solarity Rust Style Guide

This document is the authoritative Rust engineering standard for this repository. It is intentionally normative: **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, and **MAY** have their usual requirements-language meanings.

The repository follows these authorities, in descending order:

1. This guide and accepted repository architecture decisions.
2. The [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/).
3. The [Rust Style Guide](https://doc.rust-lang.org/style-guide/), as enforced by stable `rustfmt`.
4. Idiomatic usage documented by the Rust standard library.

Where the authorities disagree, the higher item wins. An exception MUST be local, documented with its reason, and approved in review. Repetition of an exception means the rule or design should be reconsidered.

## 1. Toolchain and required checks

- The workspace MUST pin its Rust toolchain in `rust-toolchain.toml`.
- All workspace packages MUST inherit the Rust edition and common metadata from the root `Cargo.toml`.
- Checked-in Rust code MUST use stable language and library features unless an accepted architecture decision explicitly requires nightly Rust.
- Code MUST pass these checks without warnings:

  ```text
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  cargo test --workspace --all-features
  ```

- Additional Clippy lints MUST be enabled individually and at workspace level. Broad lint groups such as `clippy::pedantic` MUST NOT be enabled wholesale.
- Lint suppressions MUST be as narrow as possible and include a comment when the reason is not immediately obvious.
- Generated code MUST be identifiable and excluded from hand-written style rules only where its generator requires that exclusion.

## 2. Formatting and source organization

- Default stable `rustfmt` output is final. Code MUST NOT use formatting tricks to fight `rustfmt`.
- Every source file MUST end with one newline and MUST NOT contain trailing whitespace.
- A file SHOULD define one primary concept. Split a file when its concepts have separate responsibilities or change for separate reasons; do not split it merely to meet a line count.
- Nontrivial areas inside a crate MUST be decomposed into folder-backed modules (`name/mod.rs` with child files or folders). Large flat `src/` directories MUST NOT be used.
- A folder's `mod.rs` is its facade: it SHOULD declare children, define the module's narrow public surface, and re-export intended entry points. Substantial implementation logic SHOULD live in focused child modules.
- A small leaf module MAY remain a single `name.rs` file when it has no children and splitting it would not improve navigation.
- Folder decomposition is a source-organization and privacy tool, not a reason to create another crate. New workspace crates MUST correspond to meaningful domain or dependency boundaries.
- Module declarations SHOULD appear before other items. Public re-exports MAY follow declarations when they form the module's intended API.
- Imports MUST be at module scope except where a narrower import materially improves clarity or is required by a trait method.
- Imports MUST NOT use glob syntax outside preludes, generated code, and exhaustively matched enums where the imported variants are unambiguous.
- Imports SHOULD refer to the defining module. Re-exported paths MAY be used when the re-export is an intentional facade API.
- Code MUST NOT depend on import order for meaning.

## 3. Naming

- Names MUST follow Rust casing:

  | Item | Style | Example |
  | --- | --- | --- |
  | Crates and modules | `snake_case` | `realm_auth` |
  | Functions, methods, variables, fields | `snake_case` | `decode_header` |
  | Types, traits, enum variants | `UpperCamelCase` | `RealmSession` |
  | Constants and statics | `SCREAMING_SNAKE_CASE` | `MAX_PACKET_SIZE` |
  | Lifetimes | short `snake_case` | `'a`, `'session` |

- Names MUST describe domain meaning, not implementation history. Names such as `thing`, `data`, `manager`, `helper`, `util`, and `misc` SHOULD NOT be used without a narrower domain qualifier.
- Abbreviations count as words: use `HttpClient`, `NpcState`, and `parse_guid`, not `HTTPClient`, `NPCState`, or `parse_GUID`.
- Getters MUST use the field or concept name (`session.id()`), not a `get_` prefix. Use `get` only for collection-style lookup semantics.
- Predicates SHOULD begin with `is_`, `has_`, `can_`, or `should_` when that makes the call read naturally.
- Iterator methods MUST follow standard vocabulary: `iter`, `iter_mut`, and `into_iter`.
- Conversion names MUST preserve standard semantics:

  - `as_` is cheap and borrowed-to-borrowed;
  - `to_` may allocate or compute and leaves the source usable;
  - `into_` consumes the source.

- Builder setters SHOULD use the field name. Mutating setters SHOULD use `set_` when both forms exist or mutation would otherwise be unclear.
- Units MUST be encoded in names or types when confusion is possible: `timeout_ms`, `Duration`, or a domain newtype, never an unexplained integer.
- Names MUST NOT encode type information already obvious from the declaration.

## 4. Types and API design

- APIs MUST use types to represent domain distinctions. Distinct identifiers, states, units, and protocol values SHOULD use enums or newtypes instead of interchangeable primitives.
- Invalid states SHOULD be unrepresentable. Construction MUST validate every invariant the type promises.
- Boolean parameters MUST NOT be used when the meaning at a call site is not self-evident. Use an enum, options type, or named builder method.
- Public fields SHOULD be avoided. A type MAY expose fields only when it is a transparent data value with no hidden invariant.
- Public APIs MUST be the smallest surface needed by their consumers. `pub(crate)` and private visibility are preferred over `pub`.
- Public and cross-module APIs MUST accept borrowed values when ownership is unnecessary. They MUST NOT force allocations solely for caller convenience.
- APIs SHOULD accept the least restrictive useful input (`&str`, `&Path`, or a slice) and return concrete owned types.
- Return-position `impl Trait` SHOULD be used when callers do not need the concrete implementation type. Public input-position `impl Trait` SHOULD be used only when generic call semantics are intended.
- Trait bounds SHOULD appear where they are introduced. Move them to a `where` clause when that is more readable or when bounds are nontrivial.
- Traits MUST model a coherent capability. Traits MUST NOT be created only to mock one concrete type; first prefer testing through a real module boundary.
- Existing standard traits (`From`, `TryFrom`, `AsRef`, `Display`, `Error`, iterator traits) MUST be preferred over equivalent custom methods.
- `Deref` and `DerefMut` MUST NOT be used to simulate inheritance or expose an inner type's API accidentally.
- APIs MUST NOT return references tied to locks or temporary implementation details when a stable value can be returned instead.

## 5. Ownership, allocation, and performance

- Ownership transfers MUST be intentional and visible in the API.
- `clone()` MUST NOT be used solely to silence a borrow-checker error. A clone in a nontrivial or repeated path SHOULD have an evident ownership reason.
- `Arc`, `Rc`, `Mutex`, and `RwLock` MUST NOT be default design choices. Their use MUST correspond to actual shared ownership or synchronization needs.
- Collections SHOULD be sized from known bounds when doing so is simple and materially avoids repeated allocation.
- Performance changes MUST preserve behavior and SHOULD be supported by a benchmark or profile when their benefit is not obvious.
- Unsafe code MUST NOT be introduced when a practical safe design exists. Every `unsafe` block MUST have an adjacent `// SAFETY:` comment that states the invariant making the operation valid. Unsafe abstractions MUST expose a safe boundary and MUST be tested at that boundary.

## 6. Errors, panics, and stock behavior

- **Stock behavior is the behavioral specification** unless a deliberate divergence is documented and approved.
- Unknown stock behavior MUST be researched against the pinned client executable, its disassembly/decompilation, or exact stock data before implementation. Uncertainty MUST NOT be resolved with a plausible guess.
- A stock-parity implementation MUST identify its evidence with an executable address, recovered stock symbol/path, exact data-format rule, or focused external test. If the available evidence does not bound the behavior, leave it unimplemented and document the research gap.
- Code MUST NOT invent compatibility paths, recovery behavior, retries, substitutions, guessed defaults, or silent fallbacks that stock does not use.
- Missing, malformed, or unsupported stock input MUST fail in the same observable way stock fails, unless a documented product requirement says otherwise.
- A fallback that stock does use MUST identify the stock behavior it preserves in a comment, test name, fixture, or architecture decision.
- Recoverable failures MUST use `Result`. Absence that is normal and expected SHOULD use `Option`.
- Errors MUST add useful context at subsystem boundaries without duplicating the same message at every call layer.
- Errors exposed across a module boundary MUST be stable enough for consumers to handle without parsing display text.
- Production code MUST NOT use `unwrap`, `expect`, `panic!`, `todo!`, or `unimplemented!` for reachable input or environmental failures.
- Panics MAY enforce an internal invariant whose violation is a programming defect. An `expect` message MUST state the violated invariant, not merely say that a value was missing.
- Error messages MUST be lowercase sentence fragments without trailing punctuation unless they contain multiple sentences.
- Failures MUST NOT be both logged and returned at the same abstraction layer. The layer that decides the failure is terminal owns the log entry.

## 7. Control flow and state

- Prefer early returns to deeply nested control flow.
- Exhaustive matching is preferred. A wildcard match MUST NOT hide future domain states that require a decision.
- State transitions SHOULD be expressed through methods or explicit transition types, not scattered field mutation.
- Mutable scope MUST be kept as narrow as practical.
- Iterator chains SHOULD be used when they make the transformation clearer. Use an ordinary loop when it better exposes branching, state, or errors.
- Cleverness is not a design goal. A reader familiar with Rust and the domain SHOULD be able to follow the code without mentally executing type tricks.

## 8. Async and concurrency

- Async code MUST use structured ownership: every spawned task needs a clear owner, shutdown path, and error-handling policy.
- Detached tasks and silently discarded task results MUST NOT be used.
- A synchronous mutex or read/write guard MUST NOT be held across `.await`.
- Blocking filesystem, process, or CPU-heavy work MUST NOT run on an async executor thread.
- Cancellation safety MUST be considered for operations that mutate state or consume protocol input incrementally.
- Shared mutable state SHOULD be replaced by message passing or ownership when either yields a simpler design.
- Lock ordering MUST be documented anywhere more than one lock can be held.

## 9. Modular monolith architecture

- The repository is one Cargo workspace and one deployable modular monolith.
- A domain module SHOULD be a workspace crate when it needs an enforceable dependency boundary. Small private implementation areas MAY remain Rust modules inside their owning crate.
- Each domain module MUST own its state, invariants, and domain vocabulary.
- Each module MUST expose one deliberate facade. Consumers MUST NOT reach into another module's private implementation tree.
- Dependencies MUST point toward shared foundations and MUST remain acyclic.
- Domain modules MUST NOT depend on the application composition root.
- Cross-module work MUST go through typed module APIs. A module MUST NOT read or mutate another module's database tables, files, caches, or internal state directly.
- Shared crates MUST contain genuinely shared stable concepts. They MUST NOT become dumping grounds for unrelated helpers.
- The composition root alone owns concrete wiring, process startup, shutdown, and top-level configuration assembly.
- Network or process boundaries MUST NOT be added between modules without an accepted architecture decision. Internal boundaries remain capable of later extraction, but extraction is not itself a goal.
- Workspace dependency versions and feature selections MUST be declared once at the workspace root and inherited by member crates where Cargo supports it.
- Dependency features MUST be minimal. New runtime dependencies MUST have a concrete use and MUST not duplicate an established workspace capability.
- Cyclic behavior MUST be resolved through orchestration, events, or moving the appropriate abstraction; dependency cycles MUST NOT be hidden behind a service locator or global state.

## 10. Documentation and comments

- This repository intentionally uses comments at a frequency comparable to a well-documented C++ codebase. Authors MUST NOT assume that idiomatic Rust's preference for self-documenting code eliminates explanatory comments.
- Every module, type, trait, and nontrivial function MUST have a leading comment or rustdoc summary, including private items. Trivial accessors and direct trait implementations MAY rely on the documented containing API.
- Public items MUST use rustdoc. Private-item comments MAY use ordinary comments when they describe implementation context rather than an API contract.
- A leading function comment MUST explain its purpose and any non-obvious preconditions, postconditions, ownership effects, state transitions, algorithm choice, or stock behavior. It MUST NOT merely restate the function name or signature.
- Non-obvious algorithm phases, protocol steps, state-machine branches, workarounds, and invariant-dependent operations MUST have nearby comments that make the reasoning reviewable without reconstructing it from the code.
- Struct fields and enum variants MUST be documented when their units, valid range, lifecycle, sentinel values, or interaction with other fields are not immediately obvious.
- Documentation MUST describe contracts, invariants, units, side effects, and stock-specific behavior rather than restating syntax.
- Public fallible functions MUST document `# Errors`. Public functions that can panic for caller-controlled reasons MUST document `# Panics`. Unsafe APIs MUST document `# Safety`.
- Examples in public rustdoc SHOULD compile as doctests when practical.
- Comments explain **why**, contracts, constraints, algorithm structure, or non-obvious stock behavior. They MUST NOT narrate what the next line already says.
- Comments MUST be maintained as part of the code they describe. A stale comment is a correctness defect and MUST be fixed in the same change that invalidates it.
- TODO comments MUST be actionable and include an issue reference when the work is intended to outlive the current change.
- Dead code MUST be removed instead of commented out.

## 11. Logging and diagnostics

- Runtime diagnostics MUST use the repository's structured logging facade. Library crates MUST NOT print directly to stdout or stderr.
- Log messages MUST be stable event descriptions; variable data belongs in structured fields.
- Secrets, credentials, session tokens, and full sensitive payloads MUST NOT be logged.
- Expected caller mistakes MUST NOT produce high-severity logs. Severity MUST reflect the operator action required.
- High-volume paths MUST avoid per-item informational logs unless explicitly needed for diagnostics.

## 12. Tests

- Tests MUST NOT be collocated with production code. Files under `src/` MUST NOT contain `#[cfg(test)] mod tests` or test-only implementations.
- Tests belong in dedicated `tests/` trees, organized by module and observable behavior. Test-only shared code belongs under those trees or in an explicit test-support crate.
- Tests SHOULD exercise public module behavior. Private implementation details MUST NOT be made public only to make them directly testable.
- A test MUST be deterministic and isolated from execution order, wall-clock timing, the public internet, and developer-machine state.
- Time, randomness, filesystem roots, and external processes MUST be controlled at a genuine boundary when a test depends on them.
- A bug fix MUST include a regression test whenever the failure can be reproduced automatically.
- Stock-compatibility tests SHOULD state the source behavior in the test name, fixture metadata, or a nearby reference.
- Tests MUST assert externally meaningful outcomes, not merely that code ran.
- Snapshot and golden-file changes MUST be reviewed as behavioral changes; they MUST NOT be accepted blindly through bulk regeneration.

## 13. Commits

Commits MUST follow [Conventional Commits](https://www.conventionalcommits.org/):

```text
<type>[optional scope][!]: <imperative summary>
```

- Allowed primary types are `feat`, `fix`, `refactor`, `perf`, `test`, `docs`, `build`, `ci`, `chore`, and `revert`.
- The scope SHOULD be the affected domain module when one is clearly primary.
- The summary MUST be imperative, lowercase, and have no trailing period.
- Breaking changes MUST use `!` and MUST explain the migration or impact in the body or a `BREAKING CHANGE:` footer.
- Every commit MUST include a body explaining the concrete problem or purpose,
  the resulting behavior, and relevant validation. Title-only commits MUST NOT
  be used. The body MUST explain motivation and constraints rather than repeat
  the diff; documentation-only changes MAY use a short purpose and review note.
- Each commit MUST be focused, buildable, and independently reviewable.
- Formatting-only changes SHOULD NOT be mixed with behavior changes unless the formatting is an unavoidable consequence of the edit.

### Product version and build identity

- Product versions MUST use three numeric components followed by a release
  stage: `a` for alpha, `b` for beta, `rc` for release candidate, or `s` for
  stable. For example, `0.0.0a` is the initial alpha version.
- The current product version MUST be read from the workspace `Cargo.toml`;
  [releasing](docs/releasing.md) records its product spelling and release scope.
  Version or stage changes MUST represent an explicit release decision.
- Every version change MUST include comprehensive patch notes covering all
  commits since the previous version change. Review the complete commit range,
  including fixes, features, internal work, validation, and numbered packages;
  do not derive release scope from a recent subset of commits or memory alone.
  Record the baseline and final included revision, account for every commit,
  and distinguish implemented behavior from partial or unwired work. Link the
  notes from the changelog and releasing guide. A version boundary MAY span
  overlapping development slices; it MUST NOT imply unverified completion.
- A separate monotonically increasing build number MUST identify numbered
  packaged Testing/release builds. It MUST NOT reset when the product version
  or release stage changes. Ordinary local compiles retain the current number.
  Package creation MUST use `scripts/build-client.ps1`; `-SkipBuild`
  installation reuses the compiled artifact's identity and reserves no number.
- User-facing identity MUST include both fields, for example
  `Solarity 0.0.0a — Build 000001`. The six-digit formatting is a minimum width,
  not a limit on the build sequence.
- Cargo's package version MUST use a valid SemVer encoding of the product
  version; the user-facing release suffix MUST retain the convention above.
- Product identity MUST remain independent of the original client's protocol
  and archive compatibility identifiers.

Examples:

```text
feat(auth): add realm login handshake
fix(assets): reject invalid archive offsets
refactor(protocol)!: replace legacy packet identifiers
```

## 14. Review checklist

Before merge, the author and reviewer MUST be able to answer yes to all applicable questions:

- Does the change preserve stock behavior or document the intended divergence?
- Is nontrivial crate code decomposed into folder modules with small facades?
- Are module ownership and dependency direction intact?
- Is the public surface no larger than necessary?
- Are failure behavior, ownership, and concurrency explicit?
- Do comments explain each nontrivial item, algorithm phase, and stock-specific constraint?
- Are tests outside production source and focused on observable behavior?
- Do formatting, linting, and all workspace tests pass?
- Does the commit history follow Conventional Commits and remain reviewable?
