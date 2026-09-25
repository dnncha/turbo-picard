"""Guard the adversarial generator and the stricter comparison contract."""
import unittest
from bench_bounded_markduplicates import Case, canonical_record, cases, sam_lines


class BoundedMarkduplicatesEvidenceTests(unittest.TestCase):
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
            small = Case(case.name, case.shape, 3, min(case.family_size, 5), case.remove)
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
