Share a result
==============

This page is for the point where one or two command-level trials already look
good and you want to tell other people about it without sounding vague or
overstating the claim.

What to lead with
-----------------

Lead with the narrow result you actually have:

* which Picard command was tested;
* why that command was chosen;
* what workflow shape it fits;
* what was compared;
* what happened next.

Good example:

   We tested ``MarkDuplicates`` in a ``WDL`` preprocessing task on a
   representative mitochondrial BAM shard, compared BAM output plus metrics
   against upstream Picard, and kept the original Picard path available for the
   rest of the workflow.

What not to lead with
---------------------

Do not lead with:

* blanket claims about replacing all of Picard;
* speed numbers with no command or workflow context;
* toy examples that do not resemble the workflow people care about;
* claims that skip the side-by-side comparison step.

Where to share it
-----------------

Use the channel that matches the result:

* internal chat when the immediate workflow team still needs to see it;
* a workflow or module pull request when the Picard step already lives there;
* a GitHub discussion when maintainers prefer repo-attached discussion;
* a broader community post when the result is useful outside one repository.

If the main question is just where a result belongs, the repo also includes
``packaging/outreach/channel-map.md`` and the public :doc:`community-channels`
page.

What to point people to
-----------------------

When you share a result, point readers somewhere useful:

* :doc:`first-command` if they need help choosing a trial;
* :doc:`evaluation-playbook` if they want the full evaluation flow;
* :doc:`use-cases` if they need to see where it fits;
* :doc:`faq` if the same review questions keep coming back.
* :doc:`message-examples` if they want short wording they can adapt quickly.
* :doc:`propose-it-in-a-workflow-repo` if the next step is a workflow PR or issue.
* :doc:`workflow-maintainer-checklist` if a maintainer wants a quick go/no-go screen.
* :doc:`community-channels` if the main question is which audience or venue comes next.

Smallest honest public message
------------------------------

A small useful public note usually contains:

1. the command tested;
2. the workflow context;
3. the fact that upstream Picard and ``turbo-picard`` were compared directly;
4. the next step, such as a narrow rollout, a module PR, or a request for others to try it.

If you need reusable text
-------------------------

The repository keeps short templates in ``packaging/outreach/`` for:

* ``community-post.md``
* ``github-discussion.md``
* ``module-pr-note.md``
* ``launch-plan.md``

Use those when you need wording. Use this page when you need the public-facing
rules for how to talk about the result. For public docs examples, use
:doc:`message-examples`.
