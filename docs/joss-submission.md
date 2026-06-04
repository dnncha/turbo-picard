# JOSS submission checklist

This repository includes a short JOSS-style paper draft in `paper/`.

Before submitting:

- Confirm the current public release is archived and citable.
  Current release DOI: <https://doi.org/10.5281/zenodo.20541928>.
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

Useful JOSS links:

- Author guide: <https://joss.readthedocs.io/en/latest/submitting.html>
- Paper format: <https://joss.readthedocs.io/en/latest/paper.html>
- Review criteria: <https://joss.readthedocs.io/en/latest/review_criteria.html>
