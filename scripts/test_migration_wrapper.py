"""The migration wrapper treats database URLs as literal values, never shell code."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class MigrationWrapperTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="pucksdata-wrapper-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "scripts").mkdir()
        (self.root / "bin").mkdir()
        self.wrapper = self.root / "scripts/run-migrations.sh"
        shutil.copy2(Path(__file__).with_name("run-migrations.sh"), self.wrapper)
        cargo = self.root / "bin/cargo"
        cargo.write_text("#!/usr/bin/env python3\nimport json, os, sys\n"
                         "print(json.dumps({'url': os.environ['MIGRATION_DATABASE_URL'], 'args': sys.argv[1:]}))\n")
        cargo.chmod(0o755)
        self.env = {key: value for key, value in os.environ.items()
                    if key != "MIGRATION_DATABASE_URL"}
        self.env["PATH"] = str(self.root / "bin") + os.pathsep + self.env["PATH"]

    def invoke(self):
        return subprocess.run([str(self.wrapper), "--dry-run"], env=self.env,
                              capture_output=True, text=True)

    def test_dotenv_urls_remain_literal_in_all_supported_quote_styles(self):
        url = "postgresql://user:fake$HOME#&$(touch NEVER)@localhost/test"
        for quote in ("", "'", '"'):
            with self.subTest(quote=quote):
                (self.root / ".env").write_text(f"MIGRATION_DATABASE_URL={quote}{url}{quote}\n")
                result = self.invoke()
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(result.stdout), {
                    "url": url,
                    "args": ["run", "--quiet", "--bin", "pucksdata-migrate", "--", "--dry-run"],
                })
                self.assertFalse((self.root / "NEVER").exists())

    def test_environment_url_takes_precedence(self):
        (self.root / ".env").write_text("MIGRATION_DATABASE_URL=ignored\n")
        self.env["MIGRATION_DATABASE_URL"] = "postgresql://user:fake$HOME#&@localhost/test"
        self.assertEqual(json.loads(self.invoke().stdout)["url"], self.env["MIGRATION_DATABASE_URL"])

    def test_missing_or_empty_url_fails_without_invoking_runner(self):
        for contents in (None, "MIGRATION_DATABASE_URL=\"\"\n"):
            with self.subTest(contents=contents):
                if contents is not None:
                    (self.root / ".env").write_text(contents)
                result = self.invoke()
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, "")
                self.assertIn("MIGRATION_DATABASE_URL", result.stderr)


if __name__ == "__main__":
    unittest.main()
