# Bioconda Packaging Notes

This directory contains a Bioconda-oriented recipe for local packaging smoke
tests and eventual submission.

The recipe intentionally uses:

```yaml
source:
  path: ../../..
```

That keeps packaging tests tied to the working tree before release artifacts
exist. Before opening a Bioconda PR, replace it with a tagged source archive and
`sha256`, confirm `about.home`, and replace the maintainer placeholder in
`meta.yaml`.

The build generates `THIRDPARTY.yml` with `cargo-bundle-licenses`, matching the
Bioconda Rust packaging guidance for dependency license metadata.

Local conda-build smoke:

```bash
conda build packaging/bioconda/turbo-picard
```

Bioconda-style smoke after adding the recipe to a bioconda-recipes checkout:

```bash
bioconda-utils build --docker --mulled-test turbo-picard
```
