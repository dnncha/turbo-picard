# turbo-picard-full

This Bioconda metapackage installs `turbo-picard`, upstream `picard` 3.4.0 and
`openjdk` in one environment:

```bash
mamba install -c conda-forge -c bioconda turbo-picard-full
turbo-picard doctor | grep fallback_ready
```

`turbo-picard` runs a command natively when the command and its options are
inside documented native scope. Everything else is delegated to the upstream
Picard installed by this package, found through the auto-discovery described in
`docs/fallback.rst` (`$CONDA_PREFIX/share/picard-*/picard.jar` or `picard` on
`PATH`). Every Picard 3.4.0 command name therefore works after one install;
native coverage remains command- and option-specific, and parity evidence for
switching an accelerated step still lives in `docs/parity.rst`.

The package contains no files of its own, so it is `noarch: generic` and has
no source. It does not install the `picard` shim; upstream Picard keeps the
`picard` command name. Choose `turbo-picard-picard-shim` instead only when the
`picard` name should resolve to turbo-picard, and do not install both.

`TURBO_PICARD_REQUIRE_NATIVE=1` still disables delegation for evaluation runs.
