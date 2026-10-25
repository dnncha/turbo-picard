# Comparison interface state evidence

Scope: a local instruction builder, not a job runner. Guidance: [Design with Intent / Fortify 1.7.2](https://github.com/ghaida/intent/blob/796a33594b0b25eb4fea0c21651f109f45075f28/skills/fortify/SKILL.md).

| State or event | Result and recovery | Verification |
| --- | --- | --- |
| Initial visit | Example is clearly labeled; copying reminds the reader to replace paths. | Chromium exact clipboard payload and example feedback |
| Settings edited | Earlier text remains; copy is disabled until regeneration. | Chromium stale-artifact check |
| Invalid path or missing CRAM reference | Values and previous script remain; focus moves to a field-linked error. | Node validation boundaries; Chromium focus and recovery |
| Valid generation | Current script replaces the artifact; copy is enabled. No files are opened. | Exact static/generated script match; quoted argv round-trip |
| Clipboard succeeds | The exact snapshot is copied; completion says which object was copied. | Chromium clipboard stub and exact text assertion |
| Clipboard denied | Text is selected and manual-copy instructions appear. | Chromium denial and exact selected text |
| Selection unavailable | Manual-copy instructions appear and the script block receives focus. | Chromium null-selection check |
| Copy completes after regeneration | Feedback identifies the earlier script instead of claiming the current text was copied. | Chromium deferred-promise check |
| Clipboard stalls | After eight seconds the interface offers selection/manual recovery and enables retry. The browser clipboard API does not offer cancellation. | Chromium virtual clock |
| JavaScript unavailable | Static catalogue and example remain readable; interactive actions are disabled. | Chromium JavaScript-disabled context |
| Measurement missing | Reports show Not recorded, preserving a recorded zero. | Python full and shareable report tests; no input mutation |

No work is executed, form values submitted or paths stored by these checks. The prerequisite test stops before workspace creation and proves the starter needs Python, Git and Java without requiring unused samtools. The fixed task set, shell quoting, native-only evaluator, release pin and scientific comparison contracts remain intact.

Reloading clears typed paths by design: no persistence was added. No service retry, autosave or offline queue exists because this interface has no remote submission operation. Chromium verification does not establish behavior in every browser or clipboard permission configuration.
