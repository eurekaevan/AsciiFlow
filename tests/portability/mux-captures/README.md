# Historical native mux captures

These are complete pre-fix MP4 files encoded as base64 for durable textual
storage, not freshly generated goldens. They retain native codec parameters,
extradata, all compressed packets and packet side data. Mux-only replay opens
the materialized file and uses `avcodec_parameters_copy` / `av_packet_ref`:
no encoding or decoding generates its input packets.

| Variant | Original file SHA-256 |
|---|---|
| A | c33ac221eeb792790c2113bb9674586a05711672baa04177676750ec97b7adfa |
| B | 32ea1a2fcb11e2ac328ed726bfc10536b0032ca18078b21fa61da962dcfeafda |

Materialize with `python3 tests/portability/mux-determinism.py materialize-captures
--output target/mux-captures`. Both identities are checked before writing.
The immutable C-1 historical packet vectors remain in `stage54c1.json`; the
C-1A receipt records occurrence rates, full packet vectors and output digests.
These historical structures do not become the new deterministic regression
oracle by renaming them. The existing Tier 2 comparator is unchanged.
