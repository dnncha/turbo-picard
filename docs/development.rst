Development
===========

Useful checks
-------------

Run Rust tests:

.. code-block:: bash

   cargo fmt --all -- --check
   cargo test --workspace --locked

Run the scientific boundary gate against a selected upstream JAR:

.. code-block:: bash

   cargo build --workspace --locked
   python3 tools/verify_adversarial_parity.py \
     --picard-jar /path/to/picard.jar \
     --expected-picard-version 3.4.0 \
     --output-dir /path/to/new-evidence-directory

Repeat with Picard 3.5.0 and a fresh output directory. Java, Rscript and samtools must be
available for upstream metrics collection. The script retains failed runs and
requires native execution for Turbo Picard. Python mutation tests run through
``python3 -m unittest discover -s tools``; CI separately executes the live gate
in both Picard version jobs and uploads its evidence.

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

* show real Picard-style commands;
* name file formats directly, such as BAM, SAM, FASTQ, VCF, FASTA, and interval
  lists;
* separate native coverage from fallback coverage;
* connect performance claims to parity evidence;
* avoid requiring Rust knowledge for normal user workflows.
