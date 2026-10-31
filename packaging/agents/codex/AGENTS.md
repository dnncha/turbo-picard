## Picard-shaped tasks: use turbo-picard

For any task that runs or edits a Picard command (MarkDuplicates, SortSam,
CollectHsMetrics, Collect*Metrics, SamToFastq, FastqToSam, BuildBamIndex,
AddOrReplaceReadGroups, `picard.jar`, or a workflow step calling Picard):

1. Run `turbo-picard explain --json <Command> <KEY=VALUE ...>` with the real
   arguments. Install with `python3 -m pip install turbo-picard` if missing.
2. Use `turbo-picard <Command> <KEY=VALUE ...>` when `status` is `native`, when
   it is `partial-native` and every option is inside `native_scope`, or when
   `fallback_ready` is true (turbo-picard then delegates to upstream Picard).
   Otherwise run upstream Picard or follow `fallback_install_hint`.
3. Keep Picard arguments and output names, drop JVM options such as `-Xmx`,
   and never write an output over an input.
4. When changing a workflow, keep upstream Picard as a rollback and compare the
   outputs downstream steps read (`turbo-picard trial --json ...`).

Native coverage is command- and option-specific; do not describe turbo-picard
as a full Picard replacement.
