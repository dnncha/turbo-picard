# JOSS submission checklist

This repository includes a short JOSS-style paper draft in `paper/`.

Current status: technically prepared, but do not submit to JOSS yet. The
current JOSS pre-review screening criteria require more than six months of
public development history with active iteration. This repository's first
commit is dated 2026-05-25, so the earliest reasonable submission window is
after 2026-11-25, assuming development and public release activity continues.

Before submitting later:

- Confirm the current public release is archived and citable.
  Current release DOI: <https://doi.org/10.5281/zenodo.20541928>.
- Confirm the repository still satisfies the current JOSS pre-review gates:
  public development history, demonstrated research use, active open-source
  practices, tests, documentation, releases, contribution/support expectations,
  and sustained iteration over time.
- Confirm the paper metadata still matches the repository metadata:
  author name, independent researcher affiliation, software version, and DOI.
- Run the local paper check:

  ```bash
  python3 tools/verify_joss_paper.py
  ```

- Confirm the `JOSS Paper` GitHub Actions workflow builds `paper/paper.pdf`.
- Confirm the main CI workflow is green.
- Confirm the Bioconda PR or package status is accurately described in the
  README before submission.
- Submit through the JOSS form with:
  - repository: `https://github.com/dnncha/turbo-picard`
  - paper path: `paper/paper.md`
  - archive DOI: `10.5281/zenodo.20541928`
  - conflict of interest: none, unless that changes before submission
  - related publications: none, unless a Bioinformatics/BMC/preprint submission
    exists by then

Useful JOSS links:

- Author guide: <https://joss.readthedocs.io/en/latest/submitting.html>
- Paper format: <https://joss.readthedocs.io/en/latest/paper.html>
- Review criteria: <https://joss.readthedocs.io/en/latest/review_criteria.html>
