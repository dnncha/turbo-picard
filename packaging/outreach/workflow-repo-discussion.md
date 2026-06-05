# Workflow repo discussion post

This may be worth a narrow trial on the Picard step already used in this
workflow.

The goal is not to replace all of Picard. The goal is to test whether one
command can stay in the same workflow boundary, keep the output reviewable
against upstream Picard, and reduce wall time enough to justify a small change.

If the idea makes sense here, the next step is to choose the specific command,
run it against representative data, compare the exact downstream-consumed
outputs, and then decide whether a scoped PR is worth opening.
