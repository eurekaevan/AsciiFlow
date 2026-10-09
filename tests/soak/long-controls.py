#!/usr/bin/env python3
"""One long-progress SIGINT and one kernel EFBIG mux-write failure.

Uses a real Release measurement CLI, not an in-process test pipeline. The
failure case changes only this child process's file-size limit after >=51k
observed completed frames. No allocator or driver tuning is applied.
"""
import argparse
import json
import os
from pathlib import Path
import resource
import signal
import shutil
import subprocess
import sys
import time

from generate import sha


def latest(path):
    try:
        with path.open('rb') as stream:
            stream.seek(0, 2)
            size = stream.tell()
            stream.seek(max(0, size - 65536))
            lines = stream.read().splitlines()
        return json.loads(lines[-1])
    except (FileNotFoundError, IndexError, json.JSONDecodeError):
        return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--action', choices=('cancel', 'write-failure'), required=True)
    args = parser.parse_args()
    directory = args.output.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    binary = directory / 'production-asciiflow'
    shutil.copy2(args.binary.resolve(strict=True), binary)
    binary_sha256 = sha(binary)
    target = directory / 'output.mp4'
    report = directory / 'resources.jsonl'
    diagnostic = directory / 'diagnostic.json'
    argv = [str(binary), str(args.source.resolve(strict=True)), str(target),
            '--width', '80', '--charset', 'standard', '--font', 'builtin-8x8', '--color', 'true',
            '--audio', 'copy', '--decode', 'vaapi', '--backend', 'vulkan', '--vulkan-mapping', 'gpu',
            '--encode', 'vaapi', '--hw-device', '/dev/dri/renderD128', '--input-interop', 'on',
            '--output-interop', 'on', '--output-codec', 'hevc', '--output-bit-depth', '10',
            '--output-dynamic-range', 'sdr', '--no-progress', '--diagnostic-report', str(diagnostic)]
    env = dict(os.environ, ASCIIFLOW_RELIABILITY_REPORT=str(report))
    for name in ('ASCIIFLOW_VULKAN_VALIDATION', 'ASCIIFLOW_REQUIRE_VULKAN_VALIDATION', 'VK_INSTANCE_LAYERS'):
        env.pop(name, None)
    # exec preserves ignored SIGXFSZ. Thus a kernel write-limit violation is
    # returned to FFmpeg as EFBIG, rather than terminating without teardown.
    wrapper = [sys.executable, '-c',
               'import os,signal,sys; signal.signal(signal.SIGXFSZ,signal.SIG_IGN); os.execv(sys.argv[1],sys.argv[1:])', *argv]
    started = time.monotonic()
    trigger = None
    with (directory / 'production.log').open('x') as log:
        child = subprocess.Popen(wrapper if args.action == 'write-failure' else argv,
                                 env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            while child.poll() is None:
                row = latest(report)
                if trigger is None and row and row['frames_processed'] >= 51000:
                    trigger = {'frames_processed': row['frames_processed'], 'elapsed_seconds': time.monotonic() - started}
                    if args.action == 'cancel':
                        child.send_signal(signal.SIGINT)
                    else:
                        staging = list(directory.glob('.*asciiflow-part*'))
                        assert len(staging) == 1, staging
                        limit = staging[0].stat().st_size + 4096
                        resource.prlimit(child.pid, resource.RLIMIT_FSIZE, (limit, limit))
                        trigger['file_size_limit_bytes'] = limit
                if time.monotonic() - started > 3600:
                    raise TimeoutError('long-progress control watchdog expired')
                time.sleep(0.05)
        finally:
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
    rows = [json.loads(line) for line in report.read_text().splitlines()]
    final = rows[-1]
    selected = json.loads(diagnostic.read_text())['selected_plan']
    checks = {'trigger_after_50k': trigger is not None and trigger['frames_processed'] >= 50000,
              'expected_exit': child.returncode == (130 if args.action == 'cancel' else 1),
              'no_committed_incomplete_target': not target.exists(),
              'staging_removed': not list(directory.glob('.*asciiflow-part*')),
              'post_cleanup': final['phase'] == 'post-cleanup',
              'fd_recovered': rows[0]['fd_count'] == final['fd_count'],
              'tracked_resources_recovered': bool(final['resources']) and all(r['active_count'] == 0 for r in final['resources'].values()),
              'tracked_known_bytes_recovered': all(r.get('active_bytes') in (None, 0) for r in final['resources'].values()),
              'no_accounting_errors': not any(row['accounting_errors'] for row in rows),
              'full_gpu_plan': selected['decode'] == selected['encode'] == 'Hardware'
                  and selected['backend'] == 'Vulkan'
                  and selected['hardware_input_interop'] and selected['hardware_output_interop']}
    text = (directory / 'production.log').read_text()
    if args.action == 'write-failure':
        checks['native_write_root_preserved'] = 'File too large' in text or 'file too large' in text
    record = {'action': args.action, 'command': argv, 'exec_wrapper': wrapper if args.action == 'write-failure' else None,
              'binary_sha256': binary_sha256, 'input_sha256': sha(args.source),
              'exit_code': child.returncode, 'trigger': trigger, 'wall_seconds': time.monotonic() - started,
              'checks': checks, 'initial': rows[0], 'final': final,
              'fault_scope': 'SIGINT' if args.action == 'cancel' else 'kernel RLIMIT_FSIZE EFBIG at mux output; no actual device failure claim',
              'result': 'PASS' if all(checks.values()) else 'FAIL'}
    (directory / 'result.json').write_text(json.dumps(record, indent=2) + '\n')
    return 0 if all(checks.values()) else 1


if __name__ == '__main__':
    raise SystemExit(main())
