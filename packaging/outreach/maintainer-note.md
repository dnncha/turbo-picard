# Direct maintainer note — draft, not sent

Subject: Comparing one slow Picard step in your workflow

Hi,

I maintain Turbo Picard, which runs selected Picard commands natively in Rust
while keeping the command names and arguments familiar.

I'm looking for a pipeline owner with a Picard step that costs noticeable time
or memory. Would a side-by-side comparison on a representative shard be useful?
The goal is to check the outputs you actually depend on, not to ask you to
replace the entire workflow.

The code and compatibility scope are here:
https://github.com/dnncha/turbo-picard

The comparison stays on your machine. A mismatch or an installation problem is
as useful to me as a speedup; private data and production changes are not needed.

Thanks,
Donncha

## Editor notes — not part of the message

Personalise the opening around a specific, observed workflow bottleneck before
sending. Do not bulk-post this note, promise support time that has not been
allocated, or imply that an integration has already been accepted.

The saved 32-command suite reports 84.52x geometric-mean and 272.12x top speedup
on the documented small fixtures. Those are not predictions for the recipient's
workload, so the message deliberately does not lead with them. The next-release
repository evaluator also has bounded-chunk comparison sorting; that is not a
new performance claim about the native package and must not be advertised as a
published 0.1.12 feature.
