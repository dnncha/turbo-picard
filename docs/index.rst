turbo-picard documentation
==========================

``turbo-picard`` is for teams that already use Picard and want selected commands
to run much faster without retraining everyone or rewriting the shape of a
working pipeline.

It keeps the command shape people already know:

.. code-block:: bash

   picard MarkDuplicates I=input.bam O=marked.bam M=metrics.txt

Supported commands run natively in Rust. Commands that are not ready fail
clearly, or can run through upstream Picard when you configure a fallback. The
project is meant to be adopted one command at a time, with output comparisons,
benchmark logs, real-data checks, and citation guidance kept close to the claims
they support.

This is not a blanket claim that every Picard behavior has been rebuilt. Use the
native pieces where the documented scope and your own representative data agree,
and keep upstream Picard available for the rest.

The current saved benchmark suite reports ``32/32`` parity-checked commands,
``26.74x`` geometric mean speedup, and ``84.46x`` top speedup, but the intended
workflow is still careful switching rather than blind replacement.

Start here
----------

.. grid:: 1 1 2 2
   :gutter: 2

   .. grid-item-card:: New user
      :link: quickstart
      :link-type: doc

      Install from PyPI, check the two entrypoints, and run a first
      Picard-style command.

   .. grid-item-card:: Is this for you?
      :link: is-this-for-you
      :link-type: doc

      Decide quickly whether this is worth evaluating in your workflow at all.

   .. grid-item-card:: Pipeline owner
      :link: evaluation-playbook
      :link-type: doc

      Follow the shortest path from first interest to trial, review, and team
      rollout.

   .. grid-item-card:: Use cases
      :link: use-cases
      :link-type: doc

      See the workflow situations where this package is most likely to help.

   .. grid-item-card:: Command lookup
      :link: commands
      :link-type: doc

      See which Picard commands are native, partly native, or fallback-only.

   .. grid-item-card:: Picard vs turbo-picard
      :link: picard-vs-turbo-picard
      :link-type: doc

      See what stays the same, what changes, and when to stay with Picard.

   .. grid-item-card:: FAQ
      :link: faq
      :link-type: doc

      Get direct answers to the common evaluation and rollout questions.

   .. grid-item-card:: First command
      :link: first-command
      :link-type: doc

      Pick the best first Picard step to trial instead of guessing.

   .. grid-item-card:: Packaging
      :link: packaging
      :link-type: doc

      Understand PyPI, the optional ``picard`` shim, citation boundaries, and
      the Bioconda release path.

   .. grid-item-card:: After a trial
      :link: after-evaluation
      :link-type: doc

      Turn one good command-level result into a careful rollout and a shareable
      decision record.

   .. grid-item-card:: Share a result
      :link: share-results
      :link-type: doc

      See how to talk about a good command-level result without overclaiming.

   .. grid-item-card:: Message examples
      :link: message-examples
      :link-type: doc

      Start from short example blurbs for chat, PRs, discussions, and email.

   .. grid-item-card:: Workflow repo proposal
      :link: propose-it-in-a-workflow-repo
      :link-type: doc

      See how to bring a narrow turbo-picard change into an existing workflow repo.

   .. grid-item-card:: Maintainer checklist
      :link: workflow-maintainer-checklist
      :link-type: doc

      Use a quick checklist before opening a discussion, PR, or rollout path.

   .. grid-item-card:: Community channels
      :link: community-channels
      :link-type: doc

      Map a result to the right audience and venue once it leaves your local repo.

   .. grid-item-card:: Community targets
      :link: community-targets
      :link-type: doc

      See concrete workflow-community venues where the result is likely to land well.

   .. grid-item-card:: Channel-specific examples
      :link: channel-specific-examples
      :link-type: doc

      Start from short examples already shaped for nf-core, Seqera, workflow repos, and local team chat.

   .. grid-item-card:: Launch bundle
      :link: launch-bundle
      :link-type: doc

      Use one page to assemble the first real outreach pass from the existing materials.

   .. grid-item-card:: First target shortlist
      :link: first-target-shortlist
      :link-type: doc

      See the first concrete places to try, in order, with official community links.

.. toctree::
   :maxdepth: 2
   :caption: User Guide

   quickstart
   is-this-for-you
   first-command
   evaluation-playbook
   use-cases
   picard-vs-turbo-picard
   faq
   after-evaluation
   share-results
   message-examples
   propose-it-in-a-workflow-repo
   workflow-maintainer-checklist
   community-channels
   community-targets
   channel-specific-examples
   launch-bundle
   first-target-shortlist
   maintainer-next-steps
   adoption
   parity
   fallback
   commands
   benchmarks
   performance
   citation
   packaging
   troubleshooting

.. toctree::
   :maxdepth: 2
   :caption: Project

   development
