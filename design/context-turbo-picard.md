# Turbo Picard comparison interface

## People

Bioinformatics engineers and workflow maintainers evaluate an existing Picard task on their own infrastructure. They need to preserve required scientific outputs while deciding whether runtime, memory and maintenance costs justify a change.

They move between a documentation page, a terminal and a workflow definition. File paths may belong to a remote execution machine. They need exact command names, reference requirements and supported options. They may be reading in a second language or using a screen reader.

## Product

Turbo Picard is an MIT-licensed local CLI implementing selected Picard commands in Rust. The documentation includes a command catalogue and a browser-local script builder. The builder creates instructions; the local evaluator runs the tools and writes comparison reports.

The working scope is catalogue labels, planner fields, validation messages, clipboard feedback and result interpretation. The published positioning and visual design remain the surrounding context.

## Constraints

Native support depends on the command and options. A support badge does not validate an input file. Java delegation remains a separate execution path.

The builder offers fixed starter configurations. It checks path syntax but cannot establish that a file exists, that a reference matches or that a workflow's full option set is supported. Its generated script pins an installation and uses separate output paths.

Public-facing copy must distinguish a script created in the browser, a completed comparison and a retained workflow substitution. Existing comparator rules, JSON status values, commands and scientific terminology retain their meaning.

## Data and attention

The builder does not open input files or upload entered paths. Full execution reports may contain private paths, arguments and data identifiers. A shareable summary still needs human review before posting.

Recovery messages identify the next available action. Support and result labels state their actual scope. Progress text describes observed events; durations and success claims require evidence.

## Success

A maintainer can identify command support, enter paths for the execution machine, create and copy the intended script, and understand what remains to be checked.

They distinguish an output match under a named comparator from acceptance of a whole workflow. They can recover from invalid input or clipboard restrictions without losing their current settings.

## Open questions

- Do maintainers understand the distinction between Native and Scoped native before opening command details?
- Which report fields do workflow owners inspect before retaining a substitution?
- Does the planner's fixed configuration cover the first task maintainers want to compare?
