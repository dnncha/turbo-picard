# Turbo Picard comparison interface

## Voice

| Principle | Writing rule | Use | Avoid |
| --- | --- | --- | --- |
| Precise | Name the object and the event that occurred. | “Comparison script created.” | “Ready.” |
| Calm | State a problem and a recovery action. | “Enter a path ending in .bam or .cram.” | “Invalid input!” |
| Direct | Use the action the control performs. | “Create comparison script” | “Prove the switch” on a script-generation control |
| Scientifically exact | Attach a match to its input and comparison rule. | “The recorded comparison matched for this input.” | “Your workflow is validated.” |

Use sentence case. Preserve command names, option names and file extensions exactly. Prefer sentences under 25 words. Use technical terms where they identify a required file, option or scientific condition.

## Tone

| Context | Tone | Copy |
| --- | --- | --- |
| First use | Patient and specific | “Choose a command and enter the paths you will use on the execution machine.” |
| Scope discovery | Factual | “Native within the listed scope. Read the supported options and outputs.” |
| Field validation | Helpful | “Enter the upstream Picard JAR path, such as /opt/picard/picard.jar.” |
| Script creation | Brief and factual | “Comparison script created. Review the paths and options before running it.” |
| Output mismatch | Serious and calm | “The comparison differed. Keep the current workflow while you review the differences.” |
| Public sharing | Clear about the action | “Review the shareable summary before posting it.” |

## Terminology

| Term | Meaning | Application |
| --- | --- | --- |
| Command support | The documented implementation and option scope | Catalogue and discovery links |
| Comparison script | Instructions created for a later run | Builder, script heading and clipboard feedback |
| Comparison | Running the tools and checking specified outputs | Evaluator and terminal instructions |
| Comparison result | The recorded outcome under a named comparator | Reports and result interpretation |
| Trial | A bounded evaluation of a particular task | Evaluation programme and trial reports |
| Upstream Picard | The reference implementation | JAR field, fallback explanation and reports |
| Native execution | The Rust implementation runs the task | Execution scope and native-only policy |
| Required outputs | Files and fields consumed by the workflow | Comparison contract and adoption decision |

Retain `native`, `partial-native`, `fallback-only`, `PASS` and `FAIL` in machine-readable data. Human labels explain those values without changing them. [Execution contract](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/docs/agentic-coders.rst), [comparison contract](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/docs/compatibility-contract.rst).

## Command catalogue

| Element | Copy |
| --- | --- |
| Entry action | Find command support |
| Page heading | Find support for your Picard command |
| Introduction | Search a command, then read its supported options and output checks. |
| Search label | Command or task |
| Search example | For example, MarkDuplicates or FASTQ |
| Filter label | Execution path |
| `all` option | All commands |
| `accelerated` option | Native and scoped native |
| `fallback-only` option | Requires upstream Picard |
| `native` badge | Native implementation |
| `partial-native` badge | Native within listed scope |
| `fallback-only` badge | Upstream Picard required |
| Utility badge | Inspection utility |
| Scope guidance | Read the listed options, input formats and output checks. A support label does not validate your command or file. |
| Results announcement | {visible} of {total} entries shown. Includes inspection utilities. |
| No-results message | No commands match these filters. |
| No-results recovery | Try a shorter name or choose All commands in Execution path. |
| Installed-version guidance | Check the installed version with capabilities --json before constructing a trial. |

The Native and scoped native filter includes utility entries because it filters execution status. Utility entries retain their own badge. [Catalogue filter](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/website/assets/site.js#L69), [card generation](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/tools/build_marketing_site.py#L51).

## Comparison builder

| Element | Copy |
| --- | --- |
| Page heading | Create a Picard comparison script |
| Introduction | Choose a supported task and enter your paths. Create the script, review it, then run it on your own infrastructure. |
| Command label | Command to compare |
| Command help | This starter uses fixed options. Review them against your workflow before running the script. |
| Input label | Input BAM or CRAM path |
| Input help | Use a full path starting with / on the machine where you will run the script. |
| Input example | /data/sample.bam |
| Reference label | Reference FASTA path |
| Reference help | Required for CRAM. Use the reference that matches the input. |
| Reference example | /refs/GRCh38.fa |
| Picard label | Upstream Picard JAR path |
| Picard help | Use the upstream Picard .jar file. The turbo-picard executable is a separate tool. |
| Picard example | /opt/picard/picard.jar |
| Primary action | Create comparison script |
| Data handling | This builder does not open your files or upload the paths you enter. |
| No-JavaScript message | The interactive builder needs JavaScript. Use the example script or the command-line evaluation guide. |

Keep labels and essential help visible while typing. File paths remain editable values and examples; they do not establish that files exist. The reference field remains available for BAM and required for CRAM. [Builder fields](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/website/pages/evaluate.html#L3), [path validation](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/website/assets/site.js#L6).

## Script states

| State | Output heading | Status text | Copy action |
| --- | --- | --- | --- |
| Initial example | Example script | Example paths are shown. Replace them before creating your comparison script. | Copy example script |
| Settings changed | Example script for the initial example; otherwise Previous script | The script below uses earlier values. Create a new script before copying. | Disabled |
| Script created | Comparison script | Created in this browser. Review paths, task options and temporary storage before running it. This builder has not opened your files. | Copy script |
| Validation failed | Example script for the initial example; otherwise Previous script | Script not updated. Fix the error above, then create a new script. | Disabled |
| Script copied | Comparison script | Script copied. Review it before running. | Copy script |
| Example copied | Example script | Example script copied. Replace the example paths before running. | Copy example script |
| Clipboard unavailable; text selected | Preserve the current heading | Script selected. Use your browser's Copy command. | Copy script, or Copy example script for the initial example |
| Clipboard unavailable; selection unavailable | Preserve the current heading | Clipboard access is unavailable. Select the script text and copy it using your browser. | Preserve the current action |

A field edit invalidates copying until a script is created again. A failed generation leaves the preceding script visible. Clipboard success refers to copied text. It does not imply execution or output agreement. [State transitions and clipboard handler](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/website/assets/site.js#L52).

For single-command examples outside the builder, use **Copy command**, **Command copied. Review it before running.**, and **Command selected. Use your browser's Copy command.**

## Validation messages

| Trigger | Copy | Recovery |
| --- | --- | --- |
| Required input path is empty | Enter an input BAM or CRAM path, such as /data/sample.bam. | Input field |
| Required Picard JAR path is empty | Enter the upstream Picard JAR path, such as /opt/picard/picard.jar. | Picard field |
| CRAM input has no reference | Reference FASTA is required for CRAM. Enter its full path, such as /refs/GRCh38.fa. | Reference field |
| A supplied path does not start with `/` | Use a full path starting with / for {fieldName}. | Named field |
| A path contains a control character after trimming | The {fieldName} path contains a hidden character or line break. Paste the path on one line. | Named field |
| A path exceeds the existing 4,096-unit JavaScript string-length limit | The {fieldName} path is too long for this builder. Use a shorter path. | Named field |
| Input has another extension | Enter an input path ending in .bam or .cram. | Input field |
| Picard path has another extension | Enter the upstream Picard file path ending in .jar. | Picard field |
| Command is outside the builder's supported list | Choose a command from the list or use the full evaluation guide for another task. | Command selector or evaluation-guide link |
| Page release value is malformed | The release version on this page is invalid. Reload the page or use the command-line evaluation guide. | Page reload or evaluation-guide link |

`fieldName` is Input, Upstream Picard JAR or Reference FASTA. Store each message as a complete translatable string with named values. Keep the existing path-length limit, allowed extensions and native task selection unchanged. [Validator](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/website/assets/site.js#L6).

Display the error next to its field and connect it to that control for assistive technology. Keep entered values and the earlier script available. Use the failure status once rather than repeating the field error in a second alert.

## Comparison reports

| Recorded condition | Heading or label | Supporting copy |
| --- | --- | --- |
| Command row has `PASS` | Comparison passed | The command-specific comparison matched Picard for this input. Review required sidecars and the next workflow task before switching. |
| Command row has `FAIL` | Comparison did not pass | Inspect both exit codes and comparison details to find the cause. A failed comparison is not a speed result. |
| A tool exits before comparison completes | Comparison did not finish | Read the terminal error and inspect the retained output directory. Use a fresh directory when retrying. |
| A measurement is absent | Not recorded | Show this value for that metric; preserve other recorded measurements. |
| Only one timing per implementation exists | Single-run timings | Repeat with comparable resources before making a performance decision. |
| Shareable summary is available | Review shareable summary | Check this summary before posting it. Keep full reports and private inputs on your own infrastructure. |

Apply the PASS wording to a successful command-specific comparator. Preserve the recorded method beside the result. A support badge or successful process exit alone does not select this wording. [Evidence status](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/tools/compare_real_data.py#L1749), [report generation](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/tools/compare_real_data.py#L2409).

## Content model

| Content type | Fields | Length and display rules | Review trigger |
| --- | --- | --- | --- |
| Command support | Exact command name; machine status; human label; native scope; fallback scope; comparison link; package version | Labels target 36 characters. Preserve full scope text and literal command names. | Command matrix or compatibility changes |
| Planner field | Stable field ID; label; persistent help; example; required condition; validation keys | Labels target 36 characters; help targets 140. Long paths may wrap or scroll, but remain complete when copied. | Input requirements or starter configuration changes |
| Planner notice | State; object type; heading; body; enabled copy action | Headings target 60 characters; bodies target 240. State is based on the current generated artifact. | Generation or clipboard behaviour changes |
| Validation message | Error key; field ID; complete message; named values; recovery target | Target 180 characters. Preserve required technical limits. | A validation branch changes |
| Comparison outcome | Recorded status; comparator; input scope; tool versions; required outputs; measured resources | Show absent measurements as Not recorded. Never substitute zero. Keep the comparison method beside the result. | Comparator or report schema changes |

The command matrix owns support facts. The builder owns generation and clipboard state. The evaluator owns comparison outcomes. A copy change must not create or reinterpret a machine status.

Use complete strings with named parameters. Do not assemble sentences from translated field names or fragments. Use locale-aware number formatting for result counts. Keep machine identifiers, command names, argument keys and extensions unchanged.

Review support and report wording with the maintainer responsible for the underlying contract. Review planner wording when its interaction changes. Retire copy when its associated state or validation rule is removed.

## Open questions

- Which support label helps maintainers find an option limitation before starting a trial?
- Do maintainers understand that copying a script leaves execution and comparison work ahead?
- Which field error most often stops a first comparison?
- Does the result wording make the named comparator and remaining downstream checks clear?

## Implementation note · 25 October 2026

The website implements these labels and states. Exact runtime strings live in `website/pages/evaluate.html` and `website/assets/site.js`; copy was adjusted there for persistent field help and recovery. The evaluator formatter implements absent-measurement handling in the source checkout. The starter still pins the current released package and evaluator tag; it does not silently switch to unreleased code.

FAIL guidance was corrected during implementation: the status alone does not establish a digest difference. See the decision record and state/accessibility evidence beside this document. No conversion or comprehension improvement has been measured.
