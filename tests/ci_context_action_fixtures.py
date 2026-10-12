#!/usr/bin/env python3
"""Execute the composite action's exact Bash body against private Git fixtures.

Requires Git, Bash, Python 3, and Rust 1.96.1 (RUSTC may name its compiler).
This does not execute GitHub expression evaluation, actions/checkout, or the
marketplace toolchain installer, and does not establish provider/CI parity.
"""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
ACTION = ROOT / '.github/actions/ci-context/action.yml'
REQUEST = 'a81bc02e-dc82-4b9f-8f15-2c389a012345'
REF = f'refs/heads/decapod/preflight/{REQUEST}'
ACCEPTED = 'refs/remotes/origin/master'


def action_body():
    text = ACTION.read_text()
    marker = '      run: |\n'
    if text.count(marker) != 1:
        raise AssertionError('Fixture extractor requires exactly one Bash run block')
    lines = text.split(marker, 1)[1].splitlines()
    if any(line and not line.startswith('        ') for line in lines):
        raise AssertionError('Unexpected action run-block indentation')
    return '\n'.join(line[8:] if line else '' for line in lines) + '\n'


class ActionFixtures(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.body = action_body()
        cls.compiler = os.environ.get('RUSTC') or shutil.which('rustc')
        if not cls.compiler:
            raise RuntimeError('Install Rust 1.96.1 or set RUSTC to its rustc binary')
        cls.compiler = os.path.abspath(shutil.which(cls.compiler) or cls.compiler)
        version = subprocess.check_output([cls.compiler, '--version'], text=True)
        if version.split()[1] != '1.96.1':
            raise RuntimeError(f'Fixtures require the pinned compiler: {version.strip()}')

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='decapod-ci-action-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo = self.root / 'repo'
        self.repo.mkdir()
        self.runner = self.root / 'runner'
        self.runner.mkdir()
        self.output = self.root / 'outputs'
        self.output.touch()
        self.environment = self.root / 'environment'
        self.environment.touch()
        shimdir = self.root / 'bin'
        shimdir.mkdir()
        shim = shimdir / 'rustc'
        # Only rustup's +toolchain selection is adapted to the already-verified
        # installed compiler. Compilation and the helper are executed for real.
        shim.write_text('#!/usr/bin/env bash\nset -euo pipefail\n'
                        '[[ "$1" == +1.96.1 ]]\nshift\nexec "$CI_FIXTURE_RUSTC" "$@"\n')
        shim.chmod(0o755)
        self.env = {key: value for key, value in os.environ.items()
                    if not key.startswith('GIT_')}
        self.env.update({
            'PATH': str(shimdir) + os.pathsep + os.environ['PATH'],
            'CI_FIXTURE_RUSTC': self.compiler,
            'RUNNER_TEMP': str(self.runner),
            'GITHUB_WORKSPACE': str(self.repo),
            'GITHUB_ACTION_PATH': str(ACTION.parent),
            'GITHUB_OUTPUT': str(self.output),
            'GITHUB_ENV': str(self.environment),
            'DISPATCH_PRE_PR': 'false',
            'DISPATCH_BASE': '',
            'DISPATCH_HEAD': '',
        })
        self.git('init', '-q', '-b', 'master')
        self.git('config', 'user.name', 'Action fixture')
        self.git('config', 'user.email', 'fixture@example.invalid')
        self.write('source.rs', 'pub fn before() {}\n')
        self.base = self.commit('accepted baseline')
        self.git('update-ref', ACCEPTED, self.base)
        self.write('source.rs', 'pub fn after() {}\n')
        self.source = self.commit('original candidate')
        self.tree = self.git('rev-parse', 'HEAD^{tree}')

    def git(self, *args):
        result = subprocess.run(
            ['git', '-c', 'commit.gpgsign=false', '-c', 'core.hooksPath=/dev/null',
             '-C', str(self.repo), *args],
            env=self.env, capture_output=True, text=True, check=True)
        return result.stdout.strip()

    def write(self, path, content):
        (self.repo / path).write_text(content)

    def commit(self, message):
        self.git('add', '.')
        self.git('commit', '-qm', message)
        return self.git('rev-parse', 'HEAD')

    def snapshot(self):
        message = ('decapod CI preflight snapshot\n\nschema=1\n'
                   f'request={REQUEST}\nsource={self.source}\nbase={self.base}\n'
                   'source_ref=refs/heads/feature/ci-proof\n')
        transport = self.git('commit-tree', self.tree, '-p', self.source, '-m', message)
        self.git('checkout', '-q', '--detach', transport)
        return transport

    def run_action(self, event='push', ref='refs/heads/master', base=None, head=None,
                   expected_success=True, **overrides):
        self.output.write_text('')
        self.environment.write_text('')
        env = dict(self.env, RAW_EVENT=event, RAW_REF=ref,
                   EVENT_BASE=self.base if base is None else base,
                   EVENT_HEAD=self.source if head is None else head)
        env.update(overrides)
        result = subprocess.run(['bash', '-c', self.body], env=env,
                                capture_output=True, text=True)
        if expected_success:
            self.assertEqual(result.returncode, 0, result.stderr)
            lines = self.output.read_text().splitlines()
            records = dict(line.split('=', 1) for line in lines)
            self.assertEqual(len(lines), len(records), 'duplicate output keys')
            exported_lines = self.environment.read_text().splitlines()
            exported = dict(line.split('=', 1) for line in exported_lines)
            self.assertEqual(len(exported_lines), len(exported), 'duplicate environment keys')
            self.assertEqual(exported, {
                'SOURCE_CI_EVENT': records['event'],
                'SOURCE_CI_BASE': records['base'],
                'SOURCE_CI_HEAD': records['head'],
                'SOURCE_CI_MERGE_BASE': records['merge_base'],
                'SOURCE_CI_SNAPSHOT': records['checkout_head'],
                'SOURCE_CI_VALIDATION_HEAD': records['validation_head'],
                'SOURCE_CI_TREE': records['validation_tree'],
            })
            self.assertFalse(list(self.runner.iterdir()), 'private run directory leaked')
            return records
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.output.read_bytes(), b'', 'partial outputs escaped')
        self.assertEqual(self.environment.read_bytes(), b'', 'partial environment escaped')
        self.assertFalse(list(self.runner.iterdir()), 'private run directory leaked')
        return result

    def test_snapshot_detaches_to_original_without_hooks_config_or_branch_changes(self):
        transport = self.snapshot()
        before_config = (self.repo / '.git/config').read_bytes()
        before_branch = (self.repo / '.git/refs/heads/master').read_bytes()
        marker = self.root / 'hook-was-executed'
        hook = self.repo / '.git/hooks/post-checkout'
        hook.write_text(f'#!/bin/sh\ntouch "{marker}"\n')
        hook.chmod(0o755)
        result = self.run_action(ref=REF, head=transport, base='f' * 40,
                                 GIT_DIR='/not/the/checkout',
                                 GIT_INDEX_FILE='/not/the/index')
        self.assertEqual(result['event'], 'pull_request')
        self.assertEqual(result['trigger_event'], 'push')
        self.assertEqual(result['head'], self.source)
        self.assertEqual(result['base'], self.base)
        self.assertEqual(result['transport_head'], transport)
        self.assertEqual(result['checkout_head'], transport)
        self.assertEqual(result['validation_head'], self.source)
        self.assertEqual(result['validation_tree'], self.tree)
        self.assertEqual(result['pre_pr'], 'true')
        self.assertEqual(result['snapshot'], 'true')
        self.assertEqual(result['provider_freshness'], 'not_checked')
        self.assertEqual(self.git('rev-parse', 'HEAD'), self.source)
        self.assertFalse(marker.exists(), 'checkout hook executed')
        self.assertEqual((self.repo / '.git/config').read_bytes(), before_config)
        self.assertEqual((self.repo / '.git/refs/heads/master').read_bytes(), before_branch)
        # Positive control: this hook really would run without the action's
        # per-process override. Only the owned fixture is changed here.
        subprocess.run(['git', '-C', str(self.repo), 'checkout', '--quiet',
                        '--detach', transport], env=self.env, check=True)
        self.assertTrue(marker.exists(), 'hook fixture was not executable')

    def test_ordinary_push_preserves_its_immutable_event_range(self):
        result = self.run_action()
        self.assertEqual(result['event'], 'push')
        self.assertEqual(result['base'], self.base)
        self.assertEqual(result['head'], self.source)
        self.assertEqual(result['pre_pr'], 'false')
        self.assertNotIn('transport_head', result)
        self.assertEqual(result['validation_head'], self.source)

    def test_ordinary_pr_keeps_the_exact_synthetic_merge_checkout(self):
        self.git('checkout', '-q', '--detach', self.base)
        self.write('base-only.txt', 'new base work\n')
        new_base = self.commit('advanced target')
        self.git('merge', '-q', '--no-ff', self.source, '-m', 'synthetic PR merge')
        merge = self.git('rev-parse', 'HEAD')
        tree = self.git('rev-parse', 'HEAD^{tree}')
        result = self.run_action(event='pull_request', ref='refs/pull/42/merge',
                                 base=new_base)
        self.assertEqual(result['event'], 'pull_request')
        self.assertEqual(result['head'], self.source)
        self.assertEqual(result['merge_base'], self.base)
        self.assertEqual(result['checkout_head'], merge)
        self.assertEqual(result['validation_head'], merge)
        self.assertEqual(result['validation_tree'], tree)
        self.assertEqual(self.git('rev-parse', 'HEAD'), merge)

    def test_plain_dispatch_stays_manual_and_explicit_dispatch_gets_pr_semantics(self):
        manual = self.run_action(event='workflow_dispatch', base='')
        self.assertEqual(manual['event'], 'workflow_dispatch')
        self.assertEqual(manual['base'], '')
        self.assertEqual(manual['pre_pr'], 'false')
        explicit = self.run_action(event='workflow_dispatch', base='',
                                   DISPATCH_PRE_PR='true', DISPATCH_BASE=self.base,
                                   DISPATCH_HEAD=self.source)
        self.assertEqual(explicit['event'], 'pull_request')
        self.assertEqual(explicit['trigger_event'], 'workflow_dispatch')
        self.assertEqual(explicit['pre_pr'], 'true')
        self.assertEqual(explicit['base'], self.base)
        self.assertNotIn('transport_head', explicit)
        self.run_action(event='workflow_dispatch', base='', DISPATCH_BASE=self.base,
                        expected_success=False)

    def test_failed_snapshot_never_detaches_or_emits_context(self):
        transport = self.snapshot()
        self.write('untracked-source.rs', 'not in candidate\n')
        self.run_action(ref=REF, head=transport, expected_success=False)
        self.assertEqual(self.git('rev-parse', 'HEAD'), transport)
        (self.repo / 'untracked-source.rs').unlink()
        self.run_action(ref=REF[:-1] + '6', head=transport, expected_success=False)
        self.assertEqual(self.git('rev-parse', 'HEAD'), transport)

    def test_snapshot_rejects_advanced_accepted_tip_before_detach(self):
        transport = self.snapshot()
        self.git('update-ref', ACCEPTED, self.source)
        self.run_action(ref=REF, head=transport, expected_success=False)
        self.assertEqual(self.git('rev-parse', 'HEAD'), transport)

    def test_overrides_are_dispatch_only_and_never_promote_an_ordinary_push(self):
        self.run_action(DISPATCH_PRE_PR='true', DISPATCH_BASE=self.base,
                        expected_success=False)
        self.run_action(DISPATCH_HEAD=self.source, expected_success=False)
        self.run_action(DISPATCH_PRE_PR='TRUE', expected_success=False)
        transport = self.snapshot()
        ordinary = self.run_action(ref='refs/heads/preflight/unreserved',
                                   base=self.source, head=transport)
        self.assertEqual(ordinary['event'], 'push')
        self.assertEqual(ordinary['head'], transport)
        self.assertEqual(ordinary['pre_pr'], 'false')
        self.assertEqual(ordinary['validation_head'], transport)
        self.assertNotIn('transport_head', ordinary)

    def test_dispatch_bad_candidate_and_missing_base_fail_without_outputs(self):
        self.run_action(event='workflow_dispatch', base='', DISPATCH_PRE_PR='true',
                        DISPATCH_BASE=self.base, DISPATCH_HEAD=self.base,
                        expected_success=False)
        self.run_action(event='workflow_dispatch', base='', DISPATCH_PRE_PR='true',
                        expected_success=False)
        self.assertEqual(self.git('rev-parse', 'HEAD'), self.source)


if __name__ == '__main__':
    unittest.main(verbosity=2)
