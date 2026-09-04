How to test Turbo Picard safely
-------------------------------

Do not begin by replacing every ``picard`` invocation. Begin with one command that is expensive enough to matter and narrow enough to compare.

1. Install the explicit command
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. code-block:: bash

   python3 -m pip install turbo-picard

   turbo-picard doctor --json
   turbo-picard capabilities --json

Keep upstream Picard installed. Do not enable the optional ``picard`` shim yet.

2. Preflight the exact invocation
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. code-block:: bash

   turbo-picard explain --json MarkDuplicates \
     I=input.bam \
     O=turbo.marked.bam \
     M=turbo.metrics.txt \
     CREATE_INDEX=true

Read the native scope, delegated scope, outputs, reference requirements and caveats. If a required option is outside the documented native scope, stay with Picard or configure fallback.

3. Run both tools on representative data
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Use the same input, options, reference, compression setting and machine. Include ordinary samples and at least one awkward sample: high duplication, multiple libraries, unusual read names, CRAM, UMI or whatever is difficult in your workflow.

.. code-block:: bash

   /usr/bin/time -v java -jar picard.jar MarkDuplicates \
     I=input.bam \
     O=picard.marked.bam \
     M=picard.metrics.txt \
     CREATE_INDEX=true

   /usr/bin/time -v turbo-picard MarkDuplicates \
     I=input.bam \
     O=turbo.marked.bam \
     M=turbo.metrics.txt \
     CREATE_INDEX=true

Record wall time, CPU, peak RSS, temporary disk and output size.

4. Compare the scientific contract
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Do not stop at ``samtools quickcheck``. Compare what downstream analysis consumes:

-  duplicate flags per record;
-  duplicate and optical tags;
-  duplicate-set metadata, where enabled;
-  read groups and headers;
-  metrics rows;
-  index presence and validity;
-  sort order;
-  exit status and failure behaviour.

The Turbo Picard repository contains comparison scripts and pinned evidence bundles that show how its saved claims were produced.

5. Canary the change
~~~~~~~~~~~~~~~~~~~~

Run a small percentage of samples through both paths. Keep Picard as the automatic fallback. Promote the command only after the representative comparison passes and the resource gain matters on the real scheduler.

6. Publish the result
~~~~~~~~~~~~~~~~~~~~~

A matching result is useful. A mismatch is useful too.

Record:

-  tool versions and commit or package hashes;
-  exact commands;
-  input accession or a shareable synthetic reproducer;
-  machine and storage profile;
-  parity method;
-  wall time, peak RSS and temporary disk;
-  any unsupported option or difference.

Independent results are the evidence the project needs most.

When you should stay with Picard
--------------------------------

Stay with Picard when:

-  the exact command or option is outside Turbo Picard's documented native scope;
-  the workflow needs Picard-specific charts or incidental output formatting that has not been matched;
-  you cannot run a representative comparison;
-  a regulated or clinical change-control process has not approved the new executable;
-  the measured bottleneck is storage, alignment or variant calling rather than the Picard step;
-  the native path is not materially faster or leaner on your data.

A good acceleration tool should make “no” an easy answer.

Do not trust the headline
-------------------------

The genomics industry should not trust Turbo Picard because its website has a large speedup number.

It should trust the project when:

-  the scope of each claim is visible;
-  unsupported behaviour fails or delegates instead of disappearing;
-  the benchmark can be reproduced;
-  independent teams publish matches and mismatches;
-  production-shaped WGS, WES, CRAM, UMI and multi-library evidence accumulates;
-  the tool remains useful even when the answer is “keep Picard for this command.”

Turbo Picard's current evidence says a trial is worthwhile. It does not ask you to take the rest on faith.

Pick one expensive Picard step. Run both tools. Measure the full process. Compare the outputs that matter.

**Do not trust the benchmark headline. Trust the comparison you can rerun.**

Next step
---------

Keep upstream Picard installed, print the exact trial contract, and compare one representative command:

.. code-block:: bash

   python3 -m pip install turbo-picard
   turbo-picard trial MarkDuplicates I=input.bam O=marked.bam M=metrics.txt

Then read the :doc:`evaluation playbook <evaluation-playbook>`, inspect the :doc:`command matrix <commands>`, and share a successful match, mismatch, or adoption blocker through the `trial report form <https://github.com/dnncha/turbo-picard/issues/new?template=trial-report.yml>`__.
