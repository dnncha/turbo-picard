Citation
========

If ``turbo-picard`` helps your work, cite the archived release you used and
keep the command-level parity evidence with your analysis notes. The repository
includes a ``CITATION.cff`` file so GitHub and citation tools can produce a
standard software citation.

The project citation is for the software. It is separate from the input-data
citations used by the real-data parity checks. ``CITATION.cff`` does not cite
the benchmark inputs for you; those inputs must stay pinned to immutable source
URLs, commits or accessions, and SHA-256 hashes so a reviewer can understand
exactly what was compared.

For a methods section, include:

* the ``turbo-picard`` version or archived release;
* the upstream Picard version used for parity checks, currently Picard 3.4.0
  for the checked-in release-candidate evidence;
* the exact commands you replaced;
* whether unsupported commands used upstream Picard fallback;
* the representative data and evidence reports used to justify the switch;
* the input source URL, accession or full Git commit, and SHA-256 for each
  benchmark or validation dataset you cite.

For checked-in public real-data evidence, see :doc:`benchmarks`. For practical
pipeline testing, see :doc:`adoption`.
