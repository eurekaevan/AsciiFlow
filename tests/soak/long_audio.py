"""Existing strict AAC-copy semantics, applied to finite long-soak media."""
from fractions import Fraction
import json


def verify_audio(run, label, source, output, tracks):
    probes = []
    for side, path in (("input", source), ("output", output)):
        run.checked(label + "-audio-" + side, ["ffprobe", "-v", "error",
                    "-select_streams", "a", "-show_streams", "-show_packets",
                    "-show_data_hash", "sha256", "-of", "json", path])
        probes.append(json.loads((run.out / run.commands[-1]["log"]).read_text()))
    return compare_audio(probes, tracks, label)


def compare_audio(probes, tracks, label):
    """Compare captured probes as well as live ffprobe results."""
    assert all(len(p["streams"]) == tracks for p in probes)
    counts = []
    for before, after in zip(probes[0]["streams"], probes[1]["streams"]):
        for field in ("codec_name", "sample_rate", "channels", "channel_layout", "disposition", "tags"):
            assert before.get(field) == after.get(field), (label, field)

        def packets(probe, stream):
            tb = Fraction(stream["time_base"])
            return [(p["data_hash"], int(p["size"]),
                     *(int(p[k]) * tb for k in ("pts", "dts", "duration")))
                    for p in probe["packets"] if p["stream_index"] == stream["index"]]

        original, copied = packets(probes[0], before), packets(probes[1], after)
        assert original and original == copied, label + ": strict audio packet identity"
        counts.append(len(copied))
    return {"oracle": "strict payload/timestamps/stream metadata", "result": "PASS", "packet_counts": counts}
