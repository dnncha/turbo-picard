Development
===========

Useful checks
-------------

Run Rust tests:

.. code-block:: bash

   cargo test --workspace

Run command matrix validation:

.. code-block:: bash

   python3 tools/verify_command_matrix.py

Run documentation locally:

.. code-block:: bash

   python3 -m venv .venv-docs
   . .venv-docs/bin/activate
   pip install -r docs/requirements.txt
   sphinx-build -b html docs docs/_build/html

Documentation rules of thumb
----------------------------

Write for computational biologists and bioinformaticians first:

* show real Picard-shaped commands;
* name file formats directly, such as BAM, SAM, FASTQ, VCF, FASTA, and interval
  lists;
* separate native coverage from fallback coverage;
* connect performance claims to parity evidence;
* avoid requiring Rust knowledge for normal user workflows.
