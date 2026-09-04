How I would test it in a real pipeline
--------------------------------------

I would not start with the ``picard`` shim or a repo-wide search-and-replace. Pick one command whose runtime or memory use is visible in the profile.

1. Install Turbo Picard alongside Picard
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. code-block:: bash

   python3 -m pip install turbo-picard

   turbo-picard doctor --json
   turbo-picard capabilities --json

Leave upstream Picard installed. You want an easy way back.

2. Ask about the exact command
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. code-block:: bash

   turbo-picard explain --json MarkDuplicates \
     I=input.bam \
     O=turbo.marked.bam \
     M=turbo.metrics.txt \
     CREATE_INDEX=true

Read the answer before you run anything. If an option you rely on is outside native scope, either configure fallback or keep that step on Picard.

3. Run both on data that can surprise you
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Use the same input, options, reference, compression setting and machine. Do not test only the clean sample you use in demos. Include something awkward for your workflow: high duplication, several libraries, odd read names, CRAM, UMI data, or whatever has broken assumptions before.

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

Keep wall time, CPU, peak RSS, temporary disk use and output size. A single elapsed-time number throws away too much information.

4. Compare what the next stage actually reads
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

``samtools quickcheck`` is useful, but it cannot tell you that the two tools made the same scientific decisions. For ``MarkDuplicates``, compare the duplicate flags, relevant tags, metrics, headers, sort order, index and failure behaviour that your downstream steps depend on.

The Turbo Picard repository includes command-specific comparison scripts and pinned evidence bundles. I would use those as a starting point, then add checks for the peculiarities of the real pipeline.

5. Canary it
~~~~~~~~~~~~

Run a small slice of samples through both paths. Keep Picard available. If the outputs agree and the resource saving survives contact with the real scheduler, widen the trial. If they do not, stop there.

6. Write down the result
~~~~~~~~~~~~~~~~~~~~~~~~

A useful report does not need to be flattering. Record the tool versions, exact commands, input accession or reproducer, machine and storage profile, parity method, wall time, peak RSS, temporary disk use and any mismatch.

Independent failures are as valuable to this project as independent wins. They tell me where the compatibility boundary really is.

When I would keep Picard
------------------------

I would keep the existing Picard step if the command or option is outside Turbo Picard's native scope, if I cannot run a representative comparison, or if the native path does not save enough on the real workload to justify another executable in the stack.

The same goes for regulated or clinical pipelines whose change-control process has not approved the replacement. And if profiling says the real bottleneck is storage, alignment or variant calling, changing Picard is simply work on the wrong problem.

“No change” is a perfectly good result from an evaluation.

What would make me trust it more
--------------------------------

The current benchmark is maintainer evidence. It is useful, but it is not the evidence I ultimately want.

I want production-shaped WGS, WES, CRAM, UMI and multi-library runs from people who did not write Turbo Picard. I want their exact commands and machine details. I want the mismatches left in public, not edited out of the story.

Until that body of evidence exists, the sensible claim is small: Turbo Picard has enough parity and fixture evidence to be worth testing where Picard is a measured bottleneck.

The test itself is deliberately dull. Pick one command. Run both versions. Measure the process. Compare the outputs. If Turbo Picard wins on your workload without changing what matters, keep it. If it does not, keep Picard.

Next step
---------

Keep upstream Picard installed and print the exact trial contract:

.. code-block:: bash

   python3 -m pip install turbo-picard
   turbo-picard trial MarkDuplicates I=input.bam O=marked.bam M=metrics.txt

Then read the :doc:`evaluation playbook <evaluation-playbook>`, inspect the :doc:`command matrix <commands>`, and share a match, mismatch or adoption blocker through the `trial report form <https://github.com/dnncha/turbo-picard/issues/new?template=trial-report.yml>`__.
