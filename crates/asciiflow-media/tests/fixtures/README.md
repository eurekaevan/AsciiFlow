# Failure-semantics fixtures

These intentionally tiny files exercise decoder EOF and malformed-media
behavior without depending on a GPU. `generate.sh` records their reproducible
FFmpeg construction. Tests assert semantic outcomes and frame counts, not exact
encoded bytes, so regenerating them with another compatible FFmpeg/libx264
version does not redefine output parity.

`truncated-tail.mp4`, `truncated-probe.mp4`, and `corrupt-packet.mp4` are
deliberately invalid. Do not use them as successful conversion references.
