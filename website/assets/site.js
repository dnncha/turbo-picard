/* Progressive enhancement only: content is server-readable without this file. */
(function () {
  'use strict';
  const COMMANDS = new Set(['MarkDuplicates', 'SortSam', 'SamToFastq', 'CollectMultipleMetrics']);
  function quote(value) { return "'" + String(value).replace(/'/g, "'\"'\"'") + "'"; }
  class TrialInputError extends Error {
    constructor(message, fieldId, code) {
      super(message); this.name = 'TrialInputError'; this.fieldId = fieldId; this.code = code;
    }
  }
  const PATH_ERRORS = {
    input: {
      field: 'trial-input',
      required: 'Enter an input BAM or CRAM path, such as /data/sample.bam.',
      relative: 'Use a full path starting with / for Input.',
      length: 'The Input path is too long for this builder. Use a shorter path.',
      controls: 'The Input path contains a hidden character or line break. Paste the path on one line.'
    },
    jar: {
      field: 'trial-jar',
      required: 'Enter the upstream Picard JAR path, such as /opt/picard/picard.jar.',
      relative: 'Use a full path starting with / for Upstream Picard JAR.',
      length: 'The Upstream Picard JAR path is too long for this builder. Use a shorter path.',
      controls: 'The Upstream Picard JAR path contains a hidden character or line break. Paste the path on one line.'
    },
    reference: {
      field: 'trial-reference',
      required: 'Reference FASTA is required for CRAM. Enter its full path, such as /refs/GRCh38.fa.',
      relative: 'Use a full path starting with / for Reference FASTA.',
      length: 'The Reference FASTA path is too long for this builder. Use a shorter path.',
      controls: 'The Reference FASTA path contains a hidden character or line break. Paste the path on one line.'
    }
  };
  function path(value, kind, required) {
    value = String(value || '').trim();
    if (!value && !required) return '';
    const messages = PATH_ERRORS[kind];
    const invalid = !value ? 'required' : !value.startsWith('/') ? 'relative' :
      value.length > 4096 ? 'length' : /[\x00-\x1f\x7f]/.test(value) ? 'controls' : null;
    if (invalid) throw new TrialInputError(messages[invalid], messages.field, kind + '.' + invalid);
    return value;
  }
  function buildTrial(values) {
    if (!COMMANDS.has(values.command)) throw new TrialInputError(
      'Choose a command from the list or use the full evaluation guide for another task.', 'trial-command', 'command.unsupported');
    if (!/^\d+\.\d+\.\d+$/.test(values.version)) throw new TrialInputError(
      'The release version on this page is invalid. Reload the page or use the command-line evaluation guide.', null, 'version.invalid');
    const input = path(values.input, 'input', true);
    if (!/\.(bam|cram)$/i.test(input)) throw new TrialInputError(
      'Enter an input path ending in .bam or .cram.', 'trial-input', 'input.extension');
    const jar = path(values.jar, 'jar', true);
    if (!/\.jar$/i.test(jar)) throw new TrialInputError(
      'Enter the upstream Picard file path ending in .jar.', 'trial-jar', 'jar.extension');
    const reference = path(values.reference, 'reference', /\.cram$/i.test(input));
    const lines = [
      '#!/usr/bin/env bash', 'set -euo pipefail',
      '# Review paths and fixed trial options before execution. No input is uploaded.',
      '# Set TMPDIR to suitable scratch storage before running, if needed.',
      'python3 -c \'import sys; assert sys.version_info >= (3, 11), "Python 3.11+ required"\'',
      'for tool in git java; do command -v "$tool" >/dev/null; done',
      'work="$(mktemp -d)"', 'printf \'Trial directory: %s\\n\' "$work"',
      'python3 -m venv "$work/venv"',
      '"$work/venv/bin/python" -m pip install --only-binary=:all: ' + quote('turbo-picard==' + values.version),
      'git clone --depth 1 --branch ' + quote('v' + values.version) + ' \\',
      '  https://github.com/dnncha/turbo-picard.git "$work/source"',
      'turbo_prefix="$("$work/venv/bin/python" -c \'import shlex,sys; from pathlib import Path; print(shlex.quote(str(Path(sys.executable).with_name("turbo-picard"))))\')"',
      '"$work/venv/bin/python" "$work/source/tools/compare_real_data.py" \\',
      '  --skip-build --commands ' + values.command + ' \\',
      '  --input-bam ' + quote(input) + ' \\',
    ];
    if (reference) lines.push('  --reference-fasta ' + quote(reference) + ' \\');
    // The runner shlex-parses this prefix; quote at BOTH the inner and outer layer.
    lines.push('  --picard-command ' + quote('java -jar ' + quote(jar)) + ' \\',
      '  --turbo-picard-command "$turbo_prefix" \\',
      '  --output-dir "$work/results" \\',
      '  --shareable-report "$work/results/shareable.md"',
      'printf \'Evidence retained in %s/results\\n\' "$work"',
      '# Compare status, scientific outputs and your downstream consumer.',
      '# One timing per side is not a repeat-run benchmark. Failed runs are retained.');
    return lines.join('\n');
  }
  if (typeof module !== 'undefined' && module.exports) module.exports = {quote, buildTrial};
  if (typeof document === 'undefined') return;

  const COPY = {
    command: {label: 'Copy command', pending: 'Copying command…', copied: 'Command copied. Review it before running.',
      selected: "Command selected. Use your browser's Copy command.", manual: 'Clipboard access is unavailable. Select the command text and copy it using your browser.'},
    commands: {label: 'Copy commands', pending: 'Copying commands…', copied: 'Commands copied. Review them before running.',
      selected: "Commands selected. Use your browser's Copy command.", manual: 'Clipboard access is unavailable. Select the commands and copy them using your browser.'},
    script: {label: 'Copy script', pending: 'Copying script…', copied: 'Script copied. Review it before running.',
      selected: "Script selected. Use your browser's Copy command.", manual: 'Clipboard access is unavailable. Select the script text and copy it using your browser.'},
    example: {label: 'Copy example script', pending: 'Copying example…', copied: 'Example script copied. Replace the example paths before running.',
      selected: "Example script selected. Use your browser's Copy command.", manual: 'Clipboard access is unavailable. Select the example script and copy it using your browser.'}
  };
  const plan = {revision: 0, kind: 'example', state: 'example'};
  const form = document.getElementById('evaluation-builder');
  const scriptSource = document.getElementById('trial-script');
  const scriptCopy = scriptSource ? scriptSource.closest('.codebox').querySelector('[data-copy]') : null;
  function copyKind(button) { return button === scriptCopy ? plan.kind : button.dataset.copyKind || 'command'; }
  function refreshCopy(button) {
    const messages = COPY[copyKind(button)];
    button.textContent = button.dataset.copying ? messages.pending : messages.label;
    button.setAttribute('aria-label', button.textContent);
    button.disabled = !!button.dataset.copying || (button === scriptCopy && !['example', 'ready'].includes(plan.state));
  }
  function oldCopyMessage() {
    return plan.state === 'ready' ? 'An earlier script was copied. Copy the current script to use the latest values.' :
      'An earlier script was copied. Create a new script for the current settings.';
  }
  document.querySelectorAll('[data-copy]').forEach(button => {
    refreshCopy(button);
    button.addEventListener('click', async () => {
      const box = button.closest('.codebox');
      const source = box.querySelector('pre code');
      const status = box.querySelector('.copy-status');
      const messages = COPY[copyKind(button)];
      const revision = plan.revision;
      const text = source.textContent;
      const isScript = button === scriptCopy;
      const changed = () => isScript && revision !== plan.revision;
      let timeout;
      button.dataset.copying = 'true'; button.setAttribute('aria-busy', 'true'); refreshCopy(button);
      status.textContent = messages.pending;
      try {
        if (!navigator.clipboard || !window.isSecureContext) throw new Error('Clipboard unavailable');
        await Promise.race([
          navigator.clipboard.writeText(text),
          new Promise((_, reject) => {timeout = setTimeout(() => reject(new Error('Clipboard timeout')), 8000);})
        ]);
        status.textContent = changed() ? oldCopyMessage() : messages.copied;
      } catch (_) {
        if (changed()) {
          status.textContent = plan.state === 'ready' ? 'Settings changed during copying. Copy the current script.' :
            'Settings changed during copying. Create a new script before copying.';
        } else {
          let selected = false;
          try {
            const selection = window.getSelection();
            if (selection) {
              const range = document.createRange(); range.selectNodeContents(source);
              selection.removeAllRanges(); selection.addRange(range); selected = true;
            }
          } catch (_) { /* Keep the manual-copy recovery available. */ }
          status.textContent = selected ? messages.selected : messages.manual;
          if (!selected) source.closest('pre').focus();
        }
      } finally {
        clearTimeout(timeout); delete button.dataset.copying; button.removeAttribute('aria-busy'); refreshCopy(button);
      }
    });
  });
  const search = document.getElementById('command-search');
  if (search) {
    const filter = document.getElementById('command-filter');
    const rows = Array.from(document.querySelectorAll('[data-command]'));
    const number = new Intl.NumberFormat(document.documentElement.lang || 'en');
    const apply = () => {
      const q = search.value.trim().toLowerCase(); let count = 0;
      rows.forEach(row => {
        const state = row.dataset.status;
        const matches = (!q || row.textContent.toLowerCase().includes(q)) &&
          (filter.value === 'all' || (filter.value === 'accelerated' ? state !== 'fallback-only' : state === filter.value));
        row.hidden = !matches; if (matches) count++;
      });
      document.getElementById('catalogue-count').textContent =
        `${number.format(count)} of ${number.format(rows.length)} entries shown. Includes inspection utilities.`;
      document.getElementById('no-commands').hidden = count !== 0;
    };
    search.addEventListener('input', apply); filter.addEventListener('change', apply); apply();
  }
  if (form) {
    const button = document.getElementById('generate-trial'); button.disabled = false;
    const fields = Array.from(form.querySelectorAll('input,select'));
    function clearErrors() {
      document.getElementById('trial-error').textContent = '';
      fields.forEach(field => {
        field.removeAttribute('aria-invalid');
        const errorId = field.id + '-error';
        const described = (field.getAttribute('aria-describedby') || '').split(/\s+/).filter(id => id && id !== errorId);
        if (described.length) field.setAttribute('aria-describedby', described.join(' '));
        else field.removeAttribute('aria-describedby');
        const error = document.getElementById(errorId);
        if (error) {error.textContent = ''; error.hidden = true;}
      });
    }
    function setState(state, message) {
      plan.state = state; form.dataset.scriptState = state;
      document.getElementById('trial-script-label').textContent = plan.kind === 'example' ? 'Example script' :
        state === 'ready' ? 'Comparison script' : 'Previous script';
      document.getElementById('trial-plan-status').textContent = message;
      refreshCopy(scriptCopy);
    }
    function updateReference() {
      document.getElementById('trial-reference').required = /\.cram$/i.test(document.getElementById('trial-input').value.trim());
    }
    const stale = () => {
      plan.revision++; clearErrors(); updateReference();
      scriptSource.closest('.codebox').querySelector('.copy-status').textContent = '';
      setState('stale', 'The script below uses earlier values. Create a new script before copying.');
    };
    // Keep paths in this page only, including when Enter is pressed in a field.
    form.addEventListener('keydown', event => {
      if (event.key === 'Enter' && !event.isComposing && event.target.tagName === 'INPUT') {event.preventDefault(); button.click();}
    });
    fields.forEach(field => {field.addEventListener('input', stale); field.addEventListener('change', stale);});
    updateReference(); form.dataset.scriptState = 'example';
    button.addEventListener('click', () => {
      plan.revision++; clearErrors();
      scriptSource.closest('.codebox').querySelector('.copy-status').textContent = '';
      try {
        const script = buildTrial({version: document.body.dataset.version,
          command: document.getElementById('trial-command').value,
          input: document.getElementById('trial-input').value,
          jar: document.getElementById('trial-jar').value,
          reference: document.getElementById('trial-reference').value});
        scriptSource.textContent = script; plan.kind = 'script';
        setState('ready', 'Created in this browser. Review paths, task options and temporary storage before running it. This builder has not opened your files.');
      } catch (e) {
        const message = e instanceof TrialInputError ? e.message :
          'The script could not be created. Try again or use the command-line evaluation guide.';
        const field = e instanceof TrialInputError && e.fieldId ? document.getElementById(e.fieldId) : null;
        if (field) {
          const errorId = field.id + '-error';
          const error = document.getElementById(errorId); error.textContent = message; error.hidden = false;
          field.setAttribute('aria-invalid', 'true');
          field.setAttribute('aria-describedby', ((field.getAttribute('aria-describedby') || '') + ' ' + errorId).trim());
        } else document.getElementById('trial-error').textContent = message;
        setState('invalid', 'Script not updated. Fix the error above, then create a new script.');
        if (field) field.focus();
      }
    });
  }
}());
