#!/usr/bin/env python3
"""Test real Git/pre-commit stages with a recording Cargo stub, never real builds.

Requires pre-commit (and its PyYAML dependency). Run: python3 scripts/test-hooks.py
All Git repositories, remotes, stub executables, and logs are temporary. The
project's PATH, hooks, target directory, and working files are never modified.
HOOK_CONFIG_UNDER_TEST optionally selects an older config for red/green checks.
"""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[1]
CONFIG = Path(os.environ.get("HOOK_CONFIG_UNDER_TEST", ROOT / ".pre-commit-config.yaml"))
CARGO_STUB = r'''#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
with open(os.environ["HOOK_TEST_LOG"], "a") as log:
    log.write(json.dumps(args) + "\n")
if args[0] == "fmt" and os.environ.get("HOOK_TEST_MUTATE_FMT"):
    path = pathlib.Path("fixture.rs")
    path.write_text(path.read_text() + "// formatter edit\n")
if os.environ.get("HOOK_TEST_FAIL") == args[0]:
    sys.exit(23)
'''


class HookPolicy(unittest.TestCase):
    def test_local_policy_is_unconditional_and_has_no_duplicate_gate(self):
        config = yaml.safe_load(CONFIG.read_text())
        local = [h for r in config["repos"] if r["repo"] == "local" for h in r["hooks"]]
        self.assertEqual([h["id"] for h in local], ["cargo-fmt", "cargo-pre-push"])
        fmt, gate = local
        self.assertEqual(fmt["entry"], "cargo fmt --all -- --check")
        self.assertEqual(fmt["stages"], ["pre-commit"])
        self.assertEqual(gate["entry"], "cargo xtask pre-push")
        self.assertEqual(gate["stages"], ["pre-push"])
        for hook in local:
            self.assertTrue(hook["always_run"])
            self.assertFalse(hook["pass_filenames"])
            for filtering in ["files", "types", "types_or", "exclude"]:
                self.assertNotIn(filtering, hook)
        self.assertTrue(gate["verbose"])


class RealGitStages(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="rff-hooks-")
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        self.repo = root / "repo"
        self.repo.mkdir()
        self.calls_file = root / "cargo.jsonl"
        bin_dir = root / "bin"
        bin_dir.mkdir()
        stub = bin_dir / "cargo"
        stub.write_text(CARGO_STUB)
        stub.chmod(0o755)
        self.env = dict(os.environ, PATH=str(bin_dir) + os.pathsep + os.environ["PATH"],
                        HOOK_TEST_LOG=str(self.calls_file),
                        GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1")
        # Never inherit a developer's hook bypass or Git repository overrides.
        for key in ["SKIP", "GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE",
                    "HOOK_TEST_FAIL", "HOOK_TEST_MUTATE_FMT"]:
            self.env.pop(key, None)
        self.run_cmd("git", "init", "-b", "main")
        self.run_cmd("git", "config", "user.name", "Hook stage fixture")
        self.run_cmd("git", "config", "user.email", "hooks@example.invalid")
        self.run_cmd("git", "config", "commit.gpgsign", "false")
        (self.repo / ".pre-commit-config.yaml").write_text(CONFIG.read_text())
        (self.repo / "fixture.rs").write_text("fn main() {}\n")
        (self.repo / "README.md").write_text("Fixture\n")
        self.run_cmd("git", "add", ".")
        # Seed an isolated repository before installing the hooks under test.
        self.run_cmd("git", "commit", "-m", "fixture base")
        self.base = self.run_cmd("git", "rev-parse", "HEAD").stdout.strip()
        remote = root / "remote.git"
        self.run_cmd("git", "init", "--bare", str(remote))
        self.run_cmd("git", "remote", "add", "origin", str(remote))
        self.run_cmd("git", "push", "-u", "origin", "main")
        self.run_cmd("pre-commit", "install", "--hook-type", "pre-commit", "--hook-type", "pre-push")

    def run_cmd(self, *args, success=True, overrides=None):
        env = dict(self.env, **(overrides or {}))
        result = subprocess.run(args, cwd=self.repo, env=env, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=120)
        if success:
            self.assertEqual(result.returncode, 0, result.stdout)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout)
        return result

    def calls(self):
        if not self.calls_file.exists():
            return []
        return [json.loads(line) for line in self.calls_file.read_text().splitlines()]

    def change_and_commit(self, name="fixture.rs"):
        with (self.repo / name).open("a") as file:
            file.write("// change\n" if name.endswith(".rs") else "\n# change\n")
        self.run_cmd("git", "add", name)
        self.run_cmd("git", "commit", "-m", "fixture change")

    def test_ordinary_commit_amend_and_push_have_distinct_stages(self):
        self.change_and_commit()
        self.run_cmd("git", "commit", "--amend", "--no-edit")
        self.assertEqual(self.calls(), [["fmt", "--all", "--", "--check"]] * 2)
        self.run_cmd("git", "push")
        self.assertEqual(self.calls()[-1], ["xtask", "pre-push"])
        self.assertEqual(len(self.calls()), 3)

    def test_config_only_and_later_docs_push_each_run_full_gate(self):
        self.change_and_commit(".pre-commit-config.yaml")
        self.run_cmd("git", "push")
        self.change_and_commit("README.md")
        self.run_cmd("git", "push")
        self.assertEqual(self.calls(), [["fmt", "--all", "--", "--check"],
                                       ["xtask", "pre-push"]] * 2)

    def test_gate_failure_blocks_push_and_retry_runs_again(self):
        self.change_and_commit("README.md")
        self.run_cmd("git", "push", success=False, overrides={"HOOK_TEST_FAIL": "xtask"})
        remote = self.run_cmd("git", "ls-remote", "origin", "refs/heads/main").stdout
        self.assertEqual(remote.split()[0], self.base)
        self.run_cmd("git", "push")
        self.assertEqual(self.calls()[1:], [["xtask", "pre-push"]] * 2)

    def test_formatter_failure_blocks_commit(self):
        self.run_cmd("git", "commit", "--allow-empty", "-m", "must fail", success=False,
                     overrides={"HOOK_TEST_FAIL": "fmt"})
        self.assertEqual(self.run_cmd("git", "rev-parse", "HEAD").stdout.strip(), self.base)
        self.assertEqual(self.calls(), [["fmt", "--all", "--", "--check"]])

    def test_formatter_edits_block_commit_until_restaged(self):
        self.run_cmd("git", "commit", "--allow-empty", "-m", "must fail", success=False,
                     overrides={"HOOK_TEST_MUTATE_FMT": "1"})
        self.assertEqual(self.run_cmd("git", "rev-parse", "HEAD").stdout.strip(), self.base)
        self.assertIn("formatter edit", (self.repo / "fixture.rs").read_text())
        self.run_cmd("git", "add", "fixture.rs")
        self.run_cmd("git", "commit", "-m", "restaged formatter edit")
        self.assertEqual(self.calls(), [["fmt", "--all", "--", "--check"]] * 2)


if __name__ == "__main__":
    if not shutil.which("pre-commit"):
        raise SystemExit("Install pre-commit before running stage integration tests")
    unittest.main(verbosity=2)
