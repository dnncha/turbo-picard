Use cases
=========

This page is for people who already know what Picard is and want to see where
``turbo-picard`` is most likely to pay off in real work.

WDL / Cromwell preprocessing pipelines
--------------------------------------

This is a strong use case when:

* ``MarkDuplicates`` or ``SortSam`` is clearly expensive;
* the task interface is already stable;
* you want to change the executable without redesigning the task.

Why it works well:

* the command shape stays familiar;
* the input and output contract is usually easy to preserve;
* one task can be trialed on representative shards before any broader change.

Best first move:

* start with :doc:`first-command`;
* then use ``packaging/workflows/markduplicates.wdl`` or
  ``packaging/workflows/sortsam.wdl``.

Nextflow / nf-core modules
--------------------------

This is a strong use case when:

* a Picard-shaped process is already isolated in a module;
* you want a runtime toggle between upstream Picard and ``turbo-picard``;
* the team wants a pinned binary or container without changing the broader process contract.

Why it works well:

* a module can keep the same inputs, outputs, and parameter surface;
* the toggle makes side-by-side review easier;
* maintainers can narrow the change to one hot step first.

Best first move:

* use ``packaging/workflows/markduplicates.nf`` or
  ``packaging/workflows/sortsam.nf``;
* then read ``packaging/workflows/nextflow-nf-core.md``.

Snakemake and shell pipelines
-----------------------------

This is a strong use case when:

* Picard is already called directly in a rule or shell stage;
* the command boundary is obvious;
* the team wants a low-drama first substitution.

Why it works well:

* the command swap is usually small;
* the output comparison is easy to keep close to the rule;
* fallback can stay available while only the checked command moves over.

Best first move:

* start with ``BuildBamIndex`` or ``SortSam`` if the team wants a low-risk first test;
* use ``packaging/workflows/Snakefile`` or the shell examples in :doc:`quickstart`.

Platform teams and shared workflow environments
-----------------------------------------------

This is a strong use case when:

* one platform supports many pipelines that all inherit the same slow Picard step;
* maintainers need evidence before changing the default behavior;
* distribution and rollback matter as much as raw speed.

Why it works well:

* the package can be tested with the explicit ``turbo-picard`` command first;
* the optional shim and fallback support a careful rollout;
* review artifacts can be kept for the teams downstream of the platform.

Best first move:

* use :doc:`evaluation-playbook`;
* then widen the rollout only after one command is boring on representative data.

When none of these are your situation
-------------------------------------

If your workflow does not already have a clear Picard bottleneck, or if the
only acceptable change is an immediate full replacement of everything Picard
does, start with :doc:`is-this-for-you` before spending more time.
