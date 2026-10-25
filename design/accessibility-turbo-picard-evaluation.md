# Comparison interface accessibility evidence

Scope: command catalogue, browser-local comparison builder and copy controls. Guidance: [Design with Intent / Include 1.7.2](https://github.com/ghaida/intent/blob/796a33594b0b25eb4fea0c21651f109f45075f28/skills/include/SKILL.md).

## Findings and implementation

- Each planner control has an explicit visible label. Help and error text sit outside that label so they do not become part of the control’s accessible name. Required input and JAR fields say required; CRAM requirements remain in persistent reference help, with the native required property updated when input changes.
- Validation focuses the relevant control, sets `aria-invalid`, and adds the inline error to `aria-describedby` while keeping its help association. Editing removes obsolete error associations without losing values.
- Script and clipboard feedback use atomic status regions. Clipboard success is announced after completion. Recovery distinguishes text selection from selection failure.
- Script blocks can receive keyboard focus. Manual-copy recovery focuses the block when selection is unavailable. Existing visible focus indicators remain.
- Copy controls have descriptive visible labels, wrap at narrow widths, and provide a minimum 44-pixel height. Field errors use text plus a border rather than color alone.
- Without JavaScript the catalogue and example script remain available. Nonfunctional interactive buttons are disabled, and the planner explains manual copying.

## Verification

`tools/check_marketing_browser.py` checks actual HTTP output in Chromium at 1440, 820, 390 and 320 pixels across all ten pages. Interaction checks cover error focus and associations, Enter without navigation, reference requirements, status feedback, manual recovery, and the JavaScript-disabled example. Reduced-motion behavior is checked as part of the existing suite.

This is targeted implementation evidence. It is not a WCAG conformance claim or a screen-reader usability study. Assistive-technology and workflow-owner sessions remain useful follow-up research.
