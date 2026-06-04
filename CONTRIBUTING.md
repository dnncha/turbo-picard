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
