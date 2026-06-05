FAQ
===

Is this trying to replace all of Picard?
----------------------------------------

No. The intended path is one command at a time, with side-by-side checks on
representative data before any wider switch.

How do we know where to start?
------------------------------

Start with the command that hurts most and is easy to compare. In practice
that usually means ``MarkDuplicates``, ``SortSam``, ``SamToFastq``, or
``BuildBamIndex``.

If the right first trial is still unclear, go to :doc:`first-command`.

What if our workflow still needs upstream Picard?
-------------------------------------------------

That is normal. The project already treats fallback as part of the practical
adoption path while coverage is mixed.

See :doc:`fallback` for the exact behavior.

What if the benchmark numbers do not match our workflow?
--------------------------------------------------------

That is exactly why the repo keeps command-level trials and real-data
comparison tooling. The public benchmark suite is useful context, not a reason
to skip checking your own representative shard.

See :doc:`evaluation-playbook` and :doc:`adoption`.

What if we depend on exact Picard-rendered plots?
-------------------------------------------------

Stay with upstream Picard for that step. The project does not claim that every
Picard chart PDF is reproduced byte-for-byte.

What is the smallest honest evaluation?
---------------------------------------

Pick one representative shard, run upstream Picard and ``turbo-picard`` on the
same command, compare the exact downstream-consumed outputs, and keep the
command lines and timings together.

If you want that flow spelled out, go to :doc:`evaluation-playbook`.

When should we stay with upstream Picard for now?
-------------------------------------------------

Stay with upstream Picard when:

* the command or option you need is outside the documented native scope;
* the workflow depends on exact Picard-rendered chart PDFs;
* you cannot run side-by-side checks on representative data;
* Picard is not actually the bottleneck.

See :doc:`is-this-for-you` and :doc:`picard-vs-turbo-picard` for the broader
decision framing.
