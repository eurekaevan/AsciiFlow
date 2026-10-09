#!/usr/bin/env python3
"""Review emitted measurement long-job artifacts without rerunning GPU work."""
import argparse
from fractions import Fraction
import json
from pathlib import Path

from generate import sha
from long_audio import compare_audio
from long_summary import from_files, verify_mux_counts
from preflight import verify_source_path


def review(directory):
    original = json.loads((directory / 'results.json').read_text())
    commands = {r['id']: r for r in original['commands']}
    records = []
    for kind, tracks in (('sdr', 1), ('pq-preserve', 0), ('pq-to-sdr', 2)):
        def captured(name):
            command = commands[name]
            assert command['exit_code'] == 0 and command['watchdog'] == 'completed', name
            return json.loads((directory / command['log']).read_text())
        production = commands[kind + '-production']
        assert production['exit_code'] == 0
        assert '完成：100000 帧' in (directory / production['log']).read_text()
        diagnostic = json.loads((directory / (kind + '-diagnostic.json')).read_text())
        identity = json.loads((directory / ('source-' + kind) / 'identity.json').read_text())
        verify_source_path(kind, identity, diagnostic)
        selected = diagnostic['selected_plan']
        assert selected['decode'] == selected['encode'] == 'Hardware' and selected['backend'] == 'Vulkan'
        assert selected['hardware_input_interop'] and selected['hardware_output_interop']
        probe = captured(kind + '-probe')
        stream = probe['streams'][0]
        assert len(probe['packets']) == 100000
        assert (stream['width'], stream['height'], stream['avg_frame_rate']) == (1920, 1080, '50/1')
        assert stream['pix_fmt'] == ('yuv420p' if kind == 'sdr' else 'yuv420p10le')
        colors = ('bt2020', 'smpte2084', 'bt2020nc') if kind == 'pq-preserve' else ('bt709', 'bt709', 'bt709')
        assert tuple(stream[k] for k in ('color_primaries', 'color_transfer', 'color_space')) == colors
        assert stream['color_range'] == 'tv'
        tb = Fraction(stream['time_base'])
        assert int(stream['duration_ts']) * tb == 2000
        for index, packet in enumerate(probe['packets']):
            assert int(packet['pts']) * tb == int(packet['dts']) * tb == Fraction(index, 50)
            assert int(packet['duration']) * tb == Fraction(1, 50)
        decode = commands[kind + '-full-decode']
        assert decode['exit_code'] == 0 and decode['watchdog'] == 'completed'
        if stream.get('nb_read_frames') is not None:
            assert int(stream['nb_read_frames']) == 100000
        else:
            assert int(stream['nb_read_packets']) == 100000
            progress = dict(line.split('=', 1) for line in (directory / decode['log']).read_text().splitlines() if '=' in line)
            assert int(progress['frame']) == 100000 and progress['progress'] == 'end'
        audio = compare_audio([captured(kind + '-audio-input'), captured(kind + '-audio-output')], tracks, kind) if tracks else None
        resources = directory / (kind + '-resources.jsonl')
        rows = [json.loads(line) for line in resources.read_text().splitlines()]
        verify_mux_counts(rows[-1], 100000, audio['packet_counts'] if audio else [])
        summary = from_files(resources, directory / production['process_samples'], production['elapsed_seconds'])
        assert sha(Path(identity['output']['path'])) == identity['output']['sha256']
        output = directory / (kind + '.mp4')
        assert not list(directory.glob('.*asciiflow-part*'))
        records.append({'kind': kind, 'result': 'PASS', 'frames': 100000, 'media_seconds': 2000,
                        'input': identity, 'production_command': production, 'stream': stream,
                        'output': {'path': str(output), 'bytes': output.stat().st_size, 'sha256': sha(output)},
                        'decode_back': decode, 'audio': audio, 'resource_summary': summary,
                        'resources_sha256': sha(resources)})
    return {'schema': 'asciiflow-long-run-independent-review-v1', 'result': 'PASS', 'records': records,
            'binary_sha256': sha(directory / 'production-asciiflow'),
            'original_runner_results_preserved': original['results'],
            'harness_correction': 'Original video-only packets_processed==frames assumption is invalid with AAC copy; exact mux packet gate is video frames plus independently compared copied audio packet counts. No output rerun, oracle relaxation or historical result overwrite.',
            'memory_gate': 'Pending human review of raw trends, not automatically sealed'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    document = review(args.directory.resolve())
    with args.output.open('x') as output:
        json.dump(document, output, indent=2)
        output.write('\n')


if __name__ == '__main__':
    main()
