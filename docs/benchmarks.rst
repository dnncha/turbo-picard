Benchmarks
==========

Benchmarks are useful only when they are reproducible and tied to parity. The
repository benchmark suite is designed to report speed while keeping command
coverage and Picard-compatible behavior visible.

Run the suite
-------------

.. code-block:: bash

   python3 tools/bench_suite.py --repeats 1 --skip-build

To refresh the static benchmark assets used by the project site:

.. code-block:: bash

   printf 'benchmark_date=%s source=python3 tools/bench_suite.py --repeats 1 --skip-build\n' "$(date +%F)" > docs/site/assets/bench-suite-output.txt
   python3 tools/bench_suite.py --repeats 1 --skip-build | tee -a docs/site/assets/bench-suite-output.txt
   python3 tools/render_benchmark_assets.py --suite-output docs/site/assets/bench-suite-output.txt
   python3 tools/verify_benchmark_log_evidence.py

Read benchmark claims carefully
-------------------------------

When you compare against upstream Picard, record:

* the exact command line;
* input size and sort order;
* Picard version;
* ``turbo-picard`` commit;
* CPU, memory, storage, and container or conda environment;
* parity result for the output surface you are measuring.

Do not generalize a benchmark from one command to another. ``MarkDuplicates``,
``SortSam``, FASTQ conversion, metrics collectors, and VCF utilities stress
different parts of the system.
