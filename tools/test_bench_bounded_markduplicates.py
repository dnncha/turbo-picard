"""Guard the adversarial generator and the stricter comparison contract."""
import unittest
import tempfile
import os
import sys
import time
import shutil
from pathlib import Path
from bench_bounded_markduplicates import Case, canonical_record, canonical_header, cases, sam_lines, read_histograms, measure


class BoundedMarkduplicatesEvidenceTests(unittest.TestCase):
    def test_run_dates_compare_exact_instants_without_masking_other_values(self):
        original = '@RG\tID:r\tLB:lib\tDT:2016-01-23T00:00:00-0500\n'
        pacific = original.replace('2016-01-23T00:00:00-0500', '2016-01-22T21:00:00-0800')
        utc = original.replace('2016-01-23T00:00:00-0500', '2016-01-23T05:00:00Z')
        self.assertEqual(canonical_header(original), canonical_header(pacific))
        self.assertEqual(canonical_header(original), canonical_header(utc))
        for changed in [original.replace('00:00:00', '00:00:01'), original.replace('LB:lib', 'LB:other'),
                        original.replace('-0500', '-0600'), original.replace('-0500', ''),
                        original.replace('00:00:00', '00:00:00.000001')]:
            self.assertNotEqual(canonical_header(original), canonical_header(changed))
        for value in ['2016-99-23T00:00:00-0500', '2016-01-23T00:00:00.0000001-0500',
                      '2016-01-23T00:00:00+0060', 'bad']:
            self.assertIn('DT:' + value, canonical_header('@RG\tID:r\tDT:' + value))
        self.assertIn('DT:2016-01-23T00:00:00-0500', canonical_header(original.replace('@RG', '@SQ')))

    def test_canonical_record_only_ignores_program_provenance_and_tag_order(self):
        core = 'read\t99\tchr1\t101\t60\t20M\t=\t201\t120\tAAAA\tIIII'
        original = core + '\tRG:Z:rg1\tRX:Z:AAAA\tDS:i:2\tDI:i:0\tDT:Z:SQ\tPG:Z:first\n'
        reordered = core + '\tDI:i:0\tRX:Z:AAAA\tRG:Z:rg1\tDT:Z:SQ\tDS:i:2\tPG:Z:second\n'
        self.assertEqual(canonical_record(original), canonical_record(reordered))
        for old, new in [('rg1', 'rg2'), ('\t99\t', '\t1123\t'), ('DS:i:2', 'DS:i:3'),
                         ('DI:i:0', 'DI:i:2'), ('DT:Z:SQ', 'DT:Z:LB'), ('RX:Z:AAAA', 'RX:Z:CCCC'),
                         ('\tAAAA\tIIII', '\tCAAA\tIIII'), ('\tIIII', '\tHHHH')]:
            with self.subTest(field=old):
                self.assertNotEqual(canonical_record(original), canonical_record(original.replace(old, new)))

    def test_malformed_record_is_rejected(self):
        with self.assertRaises(ValueError):
            canonical_record('too\tfew\tfields\n')

    def test_all_cases_have_valid_coordinate_order_and_complementary_mates(self):
        for case in cases():
            small = Case(case.name, case.shape, 3, min(case.family_size, 5), case.remove, case.paired_padding)
            lines = list(sam_lines(small))
            self.assertEqual(lines, list(sam_lines(small)))
            rows = [line.rstrip('\n').split('\t') for line in lines if not line.startswith('@')]
            previous = None
            templates = {}
            for row in rows:
                flag = int(row[1])
                coordinate = (1, 0) if row[2] == '*' else (0, int(row[3]))
                if previous is not None:
                    self.assertGreaterEqual(coordinate, previous)
                previous = coordinate
                self.assertEqual(len(row[9]), len(row[10]))
                if flag & 1:
                    group = next(tag for tag in row[11:] if tag.startswith('RG:Z:'))
                    templates.setdefault((group, row[0]), []).append(flag & 0xc0)
            for flags in templates.values():
                self.assertEqual(sorted(flags), [0x40, 0x80])

    def test_paired_cases_include_full_length_unique_templates(self):
        rows = [line.rstrip('\n').split('\t') for line in sam_lines(
            Case('paired', 'ties', 3, paired_padding=True)) if line.startswith('pair-')]
        self.assertEqual(len(rows), 6)
        for first, second in zip(rows[::2], rows[1::2]):
            self.assertEqual(first[0], second[0])
            self.assertEqual((int(first[1]), int(second[1])), (99, 147))
            self.assertEqual((first[5], second[5]), ('150M', '150M'))
            self.assertEqual(first[7], second[3])
            self.assertEqual(second[7], first[3])
            self.assertEqual((len(first[9]), len(second[9])), (150, 150))
        self.assertEqual(len({row[3] for row in rows}), 6)

    def test_histogram_comparison_preserves_columns_bins_and_counts(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'metrics.txt'
            original = '## HISTOGRAM\tjava.lang.Double\nBIN\tCoverageMult\tall_sets\n1.0\t1.000000\t5\n2.0\t1.5\t2\n'
            path.write_text(original)
            expected = read_histograms(path)
            path.write_text(original.replace('1.0\t1.000000\t5', '1\t1\t5.0'))
            self.assertEqual(read_histograms(path), expected)
            for old, new in [('all_sets', 'optical_sets'), ('\t5\n', '\t6\n'), ('2.0\t', '3.0\t')]:
                path.write_text(original.replace(old, new))
                self.assertNotEqual(read_histograms(path), expected)
            path.write_text('')
            self.assertNotEqual(read_histograms(path), expected)
            for malformed in [original.replace('\t5\n', '\n'), original.replace('1.5', 'NaN'),
                              original + '2.0\t1.5\t2\n', '## HISTOGRAM\tjava.lang.Double\n']:
                path.write_text(malformed)
                with self.assertRaises(ValueError):
                    read_histograms(path)

    @unittest.skipUnless(sys.platform.startswith('linux') and shutil.which('time'), 'GNU time on Linux required')
    def test_timeout_kills_measured_descendants(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            marker = root / 'survivor'
            descendant = "import pathlib,sys,time;time.sleep(0.5);pathlib.Path(sys.argv[1]).write_text('survived')"
            parent = "import subprocess,sys,time;subprocess.Popen([sys.executable,'-c',sys.argv[1],sys.argv[2]]);time.sleep(10)"
            result = measure([sys.executable, '-c', parent, descendant, str(marker)],
                             root / 'run', os.environ.copy(), timeout_seconds=0.1)
            self.assertEqual(result['exit_code'], 124)
            time.sleep(0.6)
            self.assertFalse(marker.exists())

    def test_shapes_retain_the_adversarial_properties(self):
        ties = [line.split('\t') for line in sam_lines(Case('ties', 'ties', 0)) if not line.startswith('@')]
        self.assertEqual(ties[0][3], ties[1][3])
        self.assertGreater(ties[0][0], ties[1][0])
        self.assertEqual(ties[-1][2], '*')
        shared = [line.split('\t') for line in sam_lines(Case('shared', 'same-library', 0)) if not line.startswith('@')]
        self.assertEqual(shared[0][0], shared[1][0])
        self.assertNotEqual(shared[0][-1], shared[1][-1])
        family = [line.split('\t') for line in sam_lines(Case('family', 'family', 0, 5)) if not line.startswith('@')]
        self.assertEqual([row[0] for row in family[:5]], sorted(row[0] for row in family[:5]))
        self.assertNotEqual(family[-1][2], '*')


if __name__ == '__main__':
    unittest.main()
