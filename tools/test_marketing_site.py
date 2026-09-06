"""Website regressions: source-backed claims, local planner and actual delivery."""
from pathlib import Path
import importlib.util
import json
import shutil
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
from tools import build_marketing_site as build
from tools import verify_marketing_site as verify


class MarketingSiteTests(unittest.TestCase):
    def test_all_pages_and_provenance(self):
        self.assertEqual(verify.validate(),[])
    def test_real_data_provenance_is_generated_from_pinned_manifest(self):
        manifest=json.loads((build.ROOT/'benchmarks/real-data/manifest.json').read_text())
        text=(build.ROOT/'docs/site/evidence/index.html').read_text()
        for dataset in manifest['datasets']:
            for key in ('id','source_commit','sha256','evidence_markdown'):
                self.assertIn(dataset[key],text)
    def test_full_real_data_release_gate_includes_evidence_page(self):
        subprocess.run(['python3','tools/verify_real_data_evidence.py','--release-ready'],
                       cwd=build.ROOT,check=True,capture_output=True)
    def test_generation_is_deterministic(self):
        self.assertEqual(build.render_all(),build.render_all())
        self.assertEqual(build.build(check=True),[])
    def test_prefix_depth(self):
        self.assertEqual(build.prefix(''),'./')
        self.assertEqual(build.prefix('commands/'),'../')
        self.assertEqual(build.prefix('solutions/bam-sorting/'),'../../')
    def test_catalogue_preserves_native_and_fallback_boundary(self):
        page=verify.Page((build.ROOT/'docs/site/commands/index.html').read_text())
        self.assertEqual(page.commands['CollectRnaSeqMetrics'],'fallback-only')
        self.assertEqual(page.commands['MarkDuplicates'],'partial-native')
        self.assertEqual(len(page.commands),126)
    def test_home_is_not_a_large_ratio_advertisement(self):
        home=(build.ROOT/'docs/site/index.html').read_text()
        self.assertNotIn('272.12x',home)
        self.assertIn('Small-fixture results',home)
        self.assertIn('evidence/',home)
    def test_site_source_never_contains_submitting_form(self):
        page=verify.Page((build.ROOT/'docs/site/evaluate/index.html').read_text())
        self.assertEqual(page.forms,0)
        self.assertTrue(all(x.get('type')=='button' for x in page.buttons))
    @unittest.skipUnless(shutil.which('node') and shutil.which('bash'),'Node and Bash required for planner contract test')
    def test_js_planner_argument_integrity(self):
        subprocess.run(['node','website/test-planner.cjs'],cwd=build.ROOT,check=True,capture_output=True)
    def test_sphinx_overlay_preserves_docs_and_runs_on_rebuild(self):
        with tempfile.TemporaryDirectory() as d:
            out=Path(d);(out/'quickstart.html').write_text('docs are preserved')
            app=SimpleNamespace(srcdir=str(build.ROOT/'docs'),outdir=str(out),builder=SimpleNamespace(name='html'))
            for _ in range(2):
                (out/'index.html').write_text('Sphinx default')
                build.deploy_to_sphinx(app,None)
                self.assertIn('Lose the wait.',(out/'index.html').read_text())
                self.assertEqual((out/'quickstart.html').read_text(),'docs are preserved')
                self.assertTrue((out/'solutions/bam-to-fastq/index.html').is_file())
    def test_failed_or_non_html_build_does_not_publish(self):
        with tempfile.TemporaryDirectory() as d:
            app=SimpleNamespace(srcdir=str(build.ROOT/'docs'),outdir=d,builder=SimpleNamespace(name='html'))
            build.deploy_to_sphinx(app,RuntimeError('failed'))
            app.builder.name='latex';build.deploy_to_sphinx(app,None)
            self.assertEqual(list(Path(d).iterdir()),[])
    def test_stale_site_refuses_deployment(self):
        app=SimpleNamespace(srcdir=str(build.ROOT/'docs'),outdir='/not-written',builder=SimpleNamespace(name='html'))
        with patch.object(build,'build',return_value=['index.html']):
            with self.assertRaisesRegex(RuntimeError,'stale'):build.deploy_to_sphinx(app,None)


if __name__=='__main__':unittest.main()
