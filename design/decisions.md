# Turbo Picard content decisions

## 2026-10-25 · Interface writing uses the published task-first position
- Decided by: Codex, default scope for the requested Articulate pass.
- Why: The user named the Articulate skill after publishing task-first positioning. The interactive catalogue, script builder and result language supply concrete interface material.
- Rejected: Reopening the headline, brand strategy or site layout; Articulate owns interface words and the published positioning already supplies context.
- Rests on: [User-approved publication](https://github.com/dnncha/turbo-picard/pull/45), [published product context](https://github.com/dnncha/turbo-picard/pull/45), [Articulate 1.7.2](https://github.com/ghaida/intent/blob/796a33594b0b25eb4fea0c21651f109f45075f28/skills/articulate/SKILL.md).

## 2026-10-25 · User actions distinguish instructions, execution and evidence
- Decided by: Codex, default content model.
- Why: The browser generates a script and copies text; the local evaluator performs comparison work. Support discovery and generated instructions cannot establish a matching scientific output.
- Rejected: Ready, verified and validated as generic completion labels; they conceal which operation completed.
- Rests on: [Builder and clipboard implementation](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/website/assets/site.js), [evaluator](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/tools/compare_real_data.py), [compatibility contract](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/docs/compatibility-contract.rst).

## 2026-10-25 · Human labels preserve machine contracts
- Decided by: Codex, default terminology policy.
- Why: Agents and workflow tooling consume existing command statuses and PASS/FAIL values. Human explanations can clarify them without altering their meaning.
- Rejected: Renaming JSON enums to match interface labels, treating every native-filter entry as a processing command, and translating FAIL into a biological regression claim.
- Rests on: [Agent decision contract](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/docs/agentic-coders.rst), [catalogue generation](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/tools/build_marketing_site.py), [command evidence](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/tools/compare_real_data.py#L1749).

## 2026-10-25 · Recovery distinguishes the actual path-validation condition
- Decided by: Codex, default error design.
- Why: The current path helper groups missing values, relative paths, excessive length and control characters into one message. A recovery instruction should match the failing condition.
- Rejected: Changing accepted values or relaxing path checks to simplify the message.
- Rests on: [Path helper](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/website/assets/site.js#L6).

## 2026-10-25 · Behaviour-dependent messages require their matching states
- Decided by: Codex, default implementation boundary.
- Why: Separating path failures requires distinct validator branches. Field-linked errors require control associations. Example-specific copy feedback requires artifact context. Selection failure needs its own guarded recovery path.
- Rejected: Replacing strings while claiming the additional states already exist, or announcing successful selection before that operation succeeds.
- Rests on: [Current generation and copy handlers](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/website/assets/site.js#L52), [current form controls](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/website/pages/evaluate.html#L3).

## 2026-10-25 · Current mechanisms set the limits of reassurance
- Decided by: Codex, default trust policy.
- Why: The planner does not open files or submit entered paths. Execution reports can contain private information. Only a reviewed shareable summary is suitable for a public-report starting point.
- Rejected: Broad claims that no data ever leaves the hosting service, all reports are safe to post, generated paths exist, or scratch capacity has been checked.
- Rests on: [Planner](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/website/assets/site.js), [evaluation page](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/website/pages/evaluate.html), [shareable report generator](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/tools/compare_real_data.py#L2469).

## 2026-10-25 · Usability effects remain research questions
- Decided by: Codex, default evidence boundary.
- Why: Source inspection establishes interface states and current strings. It does not establish how often maintainers misunderstand them or whether new wording improves completion.
- Rejected: Invented customer quotations, conversion gains, comprehension rates and completion-time promises.
- Rests on: [published evaluation requirements](https://github.com/dnncha/turbo-picard/blob/013832bac1eb4896698d87d011b531b2b77f85fb/docs/independent-evaluation.rst), [content-strategy reference](https://github.com/ghaida/intent/blob/796a33594b0b25eb4fea0c21651f109f45075f28/skills/intent/references/content-strategy.md).

## 2026-10-25 · The copy retains the current task sequence
- Decided by: Codex, default boundary for the requested writing pass.
- Why: Include owns field associations and focus recovery; Fortify owns added recovery transitions. The current screens and actions provide the writing scope.
- Rejected: Adding screens, reordering the flow or starting another design workflow while writing interface copy.
- Rests on: [Articulate skill boundaries](https://github.com/ghaida/intent/blob/796a33594b0b25eb4fea0c21651f109f45075f28/skills/articulate/SKILL.md).

## Source record

- Skill: Design with Intent, Articulate 1.7.2; source commit `796a33594b0b25eb4fea0c21651f109f45075f28`.
- Product source: Turbo Picard commit `013832bac1eb4896698d87d011b531b2b77f85fb`.
- Existing context: local `.agents/product-marketing.md`, version v2.
- Source material: catalogue markup and generation, planner markup and JavaScript, evaluator status/report code, compatibility and agent guides.
- Work products: [project context](context-turbo-picard.md), [copy deck and content model](copy-turbo-picard-evaluation.md).
- Implementation status: implemented in website sources and the evaluator’s report formatting. Generated pages are built from those sources. Publication and full CI are tracked by the implementation pull request. This does not constitute a new package release.

## 2026-10-25 · Implement recovery with the words it needs
- Decided by: Codex, following the request to complete and publish the specified work.
- Why: Field-linked messages, truthful clipboard feedback and stale-script recovery require matching behavior. Include and Fortify supply the implementation guidance without adding screens.
- Changes: Errors link to their controls and focus the first failing field. Copying snapshots the script revision. Pending, failed and delayed clipboard operations have distinct recovery. Editing fields retains the visible artifact but disables copying until regeneration.
- Privacy constraint: No browser persistence or path submission was added. The project’s explicit local-planning privacy promise takes precedence over a generic autosave suggestion.
- Rests on: [Include 1.7.2](https://github.com/ghaida/intent/blob/796a33594b0b25eb4fea0c21651f109f45075f28/skills/include/SKILL.md), [Fortify 1.7.2](https://github.com/ghaida/intent/blob/796a33594b0b25eb4fea0c21651f109f45075f28/skills/fortify/SKILL.md), [accessibility evidence](accessibility-turbo-picard-evaluation.md), [state evidence](states-turbo-picard-evaluation.md).

## 2026-10-25 · FAIL alone does not establish an output difference
- Decided by: Codex, based on evaluator source inspection.
- Why: A comparison can fail because of process execution or missing evidence. Report guidance says “comparison did not pass” and asks readers to inspect exit codes and details. It does not infer a scientific mismatch from the status alone.
- Changes: Missing, nonfinite and invalid runtime measurements render as “Not recorded”; a recorded zero remains zero. PASS/FAIL enums, comparators, digests and JSON evidence are unchanged.
- Distribution boundary: The browser starter keeps the existing released pin. Evaluator formatting in the checkout ships in a subsequent package release; the website does not claim it is already in the pinned wheel.
