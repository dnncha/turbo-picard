JOSS submission checklist
=========================

This repository includes a short JOSS-style paper draft in ``paper/``.

Current status: technically prepared, but do not submit to JOSS yet. The paper,
metadata, release archive, and paper-build workflow are in place. The missing
piece is time: current JOSS pre-review screening requires more than six months
of public development history, with active work spread across that period. This
repository's first commit is dated 2026-05-25, so the earliest reasonable
submission window is after 2026-11-25, assuming development and public release
activity continues.

This should be treated as a real gate, not a paperwork detail. Submitting before
there is enough public history would likely waste reviewer/editor time and risk
a desk rejection for a reason we can already see.

Before submitting later:

* Confirm the intended public release is archived and citable. The latest Zenodo
  archive documented here is ``v0.1.10``, DOI:
  https://doi.org/10.5281/zenodo.21511256. Confirm an archive for a later
  release before describing its DOI as current.
* Confirm the repository still satisfies the current JOSS pre-review gates:
  public development history, demonstrated research use, active open-source
  practices, tests, documentation, releases, contribution/support expectations,
  and sustained iteration over time.
* Confirm there is visible public activity after 2026-05-25: tagged releases,
  issues or PRs where useful, changelog entries, CI history, documentation
  improvements, and evidence that the software has been refined through use.
* Confirm the paper still makes honest claims. Do not add adoption, clinical,
  scientific-discovery, or general Picard-replacement claims unless there is
  public evidence to support them.
* Confirm the paper metadata still matches the repository metadata:
  author name, independent researcher affiliation, software version, and DOI.
* Run the local paper check:

  .. code-block:: bash

     python3 tools/verify_joss_paper.py

* Confirm the ``JOSS Paper`` GitHub Actions workflow builds ``paper/paper.pdf``.
* Confirm the main CI workflow is green.
* Confirm the Bioconda PR or package status is accurately described in
  :doc:`packaging` before submission.
* Submit through the JOSS form with:

  * repository: ``https://github.com/dnncha/turbo-picard``
  * paper path: ``paper/paper.md``
  * archive DOI: ``10.5281/zenodo.21511256``
  * conflict of interest: none, unless that changes before submission
  * related publications: none, unless a Bioinformatics/BMC/preprint submission
    exists by then

After acceptance, update the README, documentation, ``CITATION.cff``, and Zenodo
metadata with the JOSS paper DOI. Until then, cite the Zenodo software DOI.

Useful JOSS links:

* Author guide: https://joss.readthedocs.io/en/latest/submitting.html
* Paper format: https://joss.readthedocs.io/en/latest/paper.html
* Review criteria: https://joss.readthedocs.io/en/latest/review_criteria.html
