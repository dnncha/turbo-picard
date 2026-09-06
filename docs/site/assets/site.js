/* Progressive enhancement only: content is server-readable without this file. */
(function () {
  'use strict';
  const COMMANDS = new Set(['MarkDuplicates', 'SortSam', 'SamToFastq', 'CollectMultipleMetrics']);
  function quote(value) { return "'" + String(value).replace(/'/g, "'\"'\"'") + "'"; }
  function path(value, label, required) {
    value = String(value || '').trim();
    if (!value && !required) return '';
    if (!value.startsWith('/') || value.length > 4096 || /[\x00-\x1f\x7f]/.test(value)) {
      throw new Error(label + ' must be an absolute path without control characters or line breaks.');
    }
    return value;
  }
  function buildTrial(values) {
    if (!COMMANDS.has(values.command)) throw new Error('Choose one of the documented comparison tasks.');
    if (!/^\d+\.\d+\.\d+$/.test(values.version)) throw new Error('The release version is invalid.');
    const input = path(values.input, 'Input', true);
    if (!/\.(bam|cram)$/i.test(input)) throw new Error('Choose a BAM or CRAM input path.');
    const jar = path(values.jar, 'Picard jar', true);
    if (!/\.jar$/i.test(jar)) throw new Error('The upstream Picard path must end in .jar.');
    const reference = path(values.reference, 'Reference FASTA', /\.cram$/i.test(input));
    const lines = [
      '#!/usr/bin/env bash', 'set -euo pipefail',
      '# Review paths and fixed trial options before execution. No input is uploaded.',
      '# Set TMPDIR to suitable scratch storage before running, if needed.',
      'python3 -c \'import sys; assert sys.version_info >= (3, 11), "Python 3.11+ required"\'',
      'for tool in git java samtools; do command -v "$tool" >/dev/null; done',
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

  document.querySelectorAll('[data-copy]').forEach(button => {
    button.addEventListener('click', async () => {
      const box = button.closest('.codebox');
      const source = box.querySelector('pre code');
      const status = box.querySelector('.copy-status');
      try {
        if (!navigator.clipboard || !window.isSecureContext) throw new Error('Clipboard unavailable');
        await navigator.clipboard.writeText(source.textContent);
        status.textContent = 'Copied. Review the command before running it.';
      } catch (_) {
        const selection = window.getSelection();
        const range = document.createRange(); range.selectNodeContents(source);
        selection.removeAllRanges(); selection.addRange(range);
        status.textContent = 'Clipboard access is unavailable. Command selected: use your browser’s Copy command.';
      }
    });
  });
  const search = document.getElementById('command-search');
  if (search) {
    const filter = document.getElementById('command-filter');
    const rows = Array.from(document.querySelectorAll('[data-command]'));
    const apply = () => {
      const q = search.value.trim().toLowerCase(); let count = 0;
      rows.forEach(row => {
        const state = row.dataset.status;
        const matches = (!q || row.textContent.toLowerCase().includes(q)) &&
          (filter.value === 'all' || (filter.value === 'accelerated' ? state !== 'fallback-only' : state === filter.value));
        row.hidden = !matches; if (matches) count++;
      });
      document.getElementById('catalogue-count').textContent = count + ' of ' + rows.length + ' documented entries shown. Utility commands are included.';
      document.getElementById('no-commands').hidden = count !== 0;
    };
    search.addEventListener('input', apply); filter.addEventListener('change', apply); apply();
  }
  const form = document.getElementById('evaluation-builder');
  if (form) {
    const button = document.getElementById('generate-trial'); button.disabled = false;
    // Never submit paths, including when Enter is pressed in a field.
    form.addEventListener('keydown', event => {
      if (event.key === 'Enter' && !event.isComposing && event.target.tagName === 'INPUT') {event.preventDefault(); button.click();}
    });
    const copy = document.getElementById('trial-script').closest('.codebox').querySelector('[data-copy]');
    const stale = () => {
      copy.disabled = true;
      document.getElementById('trial-script-label').textContent = 'Previous example · regenerate after changes';
      document.getElementById('trial-plan-status').textContent = 'Settings changed. Generate a new script before copying.';
    };
    form.querySelectorAll('input,select').forEach(field => {field.addEventListener('input', stale);field.addEventListener('change', stale);});
    button.addEventListener('click', () => {
      const error = document.getElementById('trial-error'); error.textContent = '';
      try {
        const script = buildTrial({version: document.body.dataset.version,
          command: document.getElementById('trial-command').value,
          input: document.getElementById('trial-input').value,
          jar: document.getElementById('trial-jar').value,
          reference: document.getElementById('trial-reference').value});
        document.getElementById('trial-script').textContent = script;
        copy.disabled = false;
        document.getElementById('trial-script-label').textContent = 'Generated plan · not executed';
        document.getElementById('trial-plan-status').textContent = 'Script updated locally. Paths have not been inspected. Review prerequisites, task options and scratch capacity before executing.';
      } catch (e) {
        error.textContent = e.message; copy.disabled = true;
        document.getElementById('trial-plan-status').textContent = 'No new script generated. The previous example below has not changed.';
      }
    });
  }
}());
