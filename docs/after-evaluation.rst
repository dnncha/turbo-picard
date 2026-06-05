After a good trial
==================

If one ``turbo-picard`` command has already passed a representative side-by-side
check for your workflow, the next question is not whether the package looks
interesting. The next question is how to turn that result into careful adoption.

1. Write down what was actually tested
--------------------------------------

Use ``packaging/outreach/team-review-template.md`` while the details are still
fresh:

* which Picard command was tested;
* what representative input was used;
* what outputs and sidecars were compared;
* what the observed runtime difference was;
* what the recommendation is for that command.

2. Keep the scope narrow
------------------------

Do not widen the claim from one successful trial to a whole workflow family.

Good next steps:

* use the command only in the workflow path you actually checked;
* leave upstream Picard available for unsupported or unverified steps;
* queue the next command only after the first one is boring on real data.

3. Pick the right sharing path
------------------------------

Use the outreach kit based on who needs to hear about the result:

* ``packaging/outreach/slack-message.md`` for immediate teammates;
* ``packaging/outreach/email-blurb.md`` for workflow owners or platform leads;
* ``packaging/outreach/module-pr-note.md`` for a workflow or module PR;
* ``packaging/outreach/community-post.md`` or
  ``packaging/outreach/github-discussion.md`` for broader visibility.

4. Bring the right evidence
---------------------------

Lead with:

* the exact command that was tested;
* why that command was chosen;
* the workflow shape it fits;
* where the trial notes and starter files live.

Do not lead with:

* blanket claims about replacing all of Picard;
* benchmark numbers without naming the command they apply to;
* tiny toy inputs that do not resemble the real workflow context.

5. Decide whether to widen the rollout
--------------------------------------

Widen only when the first substitution is already easy to defend.

Reasonable next steps:

* try the next highest-friction Picard command in the same workflow;
* add a runtime switch or fallback path if you need a gradual rollout;
* keep the evaluation material close to the workflow repo so future changes stay reviewable.

Use ``packaging/outreach/objections.md`` if the same concerns keep resurfacing.

If you are the maintainer deciding where to take the result next, continue with
:doc:`maintainer-next-steps`.
