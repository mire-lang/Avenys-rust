import shlex
import subprocess
import sys
import unittest
from pathlib import Path


HARNESS = Path(__file__).parent / "pal_compat" / "compare.py"


class CompatibilityHarnessTests(unittest.TestCase):
    def run_case(self, reference, candidate):
        return subprocess.run(
            [
                sys.executable,
                str(HARNESS),
                "--reference",
                shlex.join([sys.executable, "-c", reference]),
                "--candidate",
                shlex.join([sys.executable, "-c", candidate]),
            ],
            capture_output=True,
            text=True,
            check=False,
        )

    def test_matching_results_pass(self):
        result = self.run_case("print('same')", "print('same')")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("compatible", result.stdout)

    def test_output_or_exit_differences_fail(self):
        result = self.run_case("print('C')", "print('Mire')")
        self.assertEqual(result.returncode, 1)
        self.assertIn("stdout differs", result.stdout)


if __name__ == "__main__":
    unittest.main()
