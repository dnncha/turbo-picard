from pathlib import Path
import sys
import tempfile
import unittest

from bench_native_duplicate_landscape import pipeline_commands, run_pipeline


class NativeLandscapeTests(unittest.TestCase):
    def test_preparation_and_coordinate_output_are_inside_both_pipelines(self):
        for tool in ('samtools', 'dupblaster'):
            commands = pipeline_commands(tool, 'dupblaster', 'samtools', Path('original.bam'),
                                         Path('result.bam'), Path('metrics'), Path('scratch'))
            self.assertEqual(commands[0][1], 'collate')
            self.assertIn('original.bam', commands[0])
            self.assertIn('result.bam', commands[-1])
            sort = next(command for command in commands if command[1] == 'sort')
            self.assertEqual(sort[sort.index('-m') + 1], '256M')
            self.assertEqual(sort[sort.index('-T') + 1], 'scratch/sort')
        with self.assertRaises(ValueError):
            pipeline_commands('unknown', '', '', Path(''), Path(''), Path(''), Path(''))

    def test_upstream_failure_is_not_masked_by_successful_last_stage(self):
        self.assertEqual(run_pipeline([[sys.executable, '-c', 'raise SystemExit(7)'],
                                       [sys.executable, '-c', 'import sys;sys.stdin.read()']]), 7)
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'result'
            self.assertEqual(run_pipeline([[sys.executable, '-c', 'print("payload")'],
                                           [sys.executable, '-c',
                                            'import pathlib,sys;pathlib.Path(sys.argv[1]).write_text(sys.stdin.read())',
                                            str(target)]]), 0)
            self.assertEqual(target.read_text(), 'payload\n')
