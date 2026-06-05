Maintainer next steps
=====================

This page is for the point where one ``turbo-picard`` command already looks
good on representative data and a maintainer needs to decide what to do next.

1. Keep the first success narrow
--------------------------------

Treat the successful trial as evidence for that command in that workflow shape,
not as proof that every Picard step is ready to switch.

Reasonable first moves:

* keep the command in the exact path you tested;
* leave upstream Picard available for everything else;
* avoid broad wording such as "we replaced Picard" when the evidence is still
  command-level.

2. Put the result somewhere reviewable
--------------------------------------

Use ``packaging/outreach/team-review-template.md`` so the result can be read by
other workflow owners later.

Useful details to keep:

* the exact command tested;
* the representative input or shard;
* the outputs and sidecars that were compared;
* the runtime difference that was observed;
* the next recommendation for that command.

3. Pick one next distribution target
------------------------------------

Do not try to do everything at once. Choose the channel that is closest to the
workflow context that already produced a good trial:

* internal chat if the immediate team still needs to see it;
* a workflow or module PR if a Picard step is already under discussion;
* a release note if users of that workflow need to know the option exists;
* a GitHub discussion or short community post if the result is useful beyond
  one local repo.

Use ``packaging/outreach/channel-map.md`` to pick the right venue,
``packaging/outreach/launch-plan.md`` for the recommended order, and
``packaging/outreach/`` for reusable text.

4. Queue the second command carefully
-------------------------------------

Only after the first command is easy to defend should you widen the rollout.

Good candidates for the second step are usually:

* the next most expensive Picard command in the same workflow;
* a command that is already easy to compare with downstream outputs;
* a command that fits the same workflow boundary and does not require a
  redesign.

5. Keep the evaluation path visible
-----------------------------------

Point future reviewers to:

* :doc:`evaluation-playbook`
* :doc:`after-evaluation`
* ``packaging/workflows/``
* ``packaging/outreach/``

That keeps later conversations grounded in the same evidence discipline instead
of restarting from generic claims.
