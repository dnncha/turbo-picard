# Contributing to turbo-picard

Thanks for taking the time to improve `turbo-picard`. The project is useful only
if its claims stay narrow, checked, and easy for pipeline owners to audit.

## Good first contributions

Helpful contributions include:

- bug reports with a small reproducible command and input shape;
- documentation fixes where the current scope is unclear;
- parity tests for an existing command or option;
- benchmark fixtures that use public, pinned, citable input data;
- small native command improvements with tests and command-matrix updates.

Before adding or widening native behavior, check
[`docs/command-matrix.yml`](docs/command-matrix.yml). The command matrix is the
public source of truth for what is native, partly native, fallback-only, or not
implemented.

## Reporting bugs

Please include:

- the `turbo-picard` version or commit;
- the upstream Picard version used for comparison, if relevant;
- the exact command line;
- whether fallback was configured;
- a small input file, public accession, or enough detail to reproduce the input
  shape;
- the expected behavior and the observed behavior.

Do not upload private clinical, human-subject, or controlled-access data. If the
problem depends on sensitive data, describe the file shape and failure mode, or
try to make a tiny synthetic reproducer.

## Pull requests

For code changes, run the relevant checks before opening a PR:

```bash
cargo fmt --all -- --check
cargo test --workspace
python3 tools/verify_command_matrix.py
python3 tools/verify_benchmarks.py
python3 tools/verify_real_data_evidence.py
```

### Expect tests and examples

Pure boundaries use Jane Street-style expect tests. The whole normalised map,
config struct, or error list is stored next to the assertion, so a changed
default or ordering rule is visible in review:

```bash
cargo test -p turbo-picard-core --test expectations
cargo test -p turbo-picard-cli --test expectations
```

After an intentional change, rewrite the inline expectations and read the
resulting diff before committing:

```bash
UPDATE_EXPECT=1 cargo test -p turbo-picard-core --test expectations
UPDATE_EXPECT=1 cargo test -p turbo-picard-cli --test expectations
```

The `tests/examples.rs` targets and the `///` examples in public APIs are
runnable usage checks. `cargo test --workspace` executes the doctests; keep the
examples small enough to read and keep them compiling.

If the change affects documentation or the paper, also run:

```bash
python3 -m sphinx -W -b html docs docs/_build/html
python3 tools/verify_joss_paper.py
```

Native command changes should normally include:

- tests for the new behavior;
- an update to `docs/command-matrix.yml`;
- documentation that says what is supported and what still falls back;
- parity evidence when the behavior is meant to match Picard.

## Style

Prefer plain explanations over broad claims. A narrow, reproducible statement is
better than a large promise. If a command has only been checked for a particular
option set or input shape, say that directly.
