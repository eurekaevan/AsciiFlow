//! Test-only, hardware-independent media regression oracle.
//! FFmpeg reads container/packet facts; AsciiFlow's software decoder supplies
//! tightly packed visible NV12/P010 frames for decoded-pixel SHA-256.

use super::h264_bitstream::{avcc_length_size, compare_avcc_packets};
use asciiflow_core::{FrameSource, PixelFormat};
use asciiflow_media::Decoder;
use ffmpeg_sys_next as ffi;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{CStr, CString},
    path::Path,
    ptr,
};

type Check<T> = Result<T, String>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamFacts {
    pub index: i32,
    pub kind: i32,
    pub codec: i32,
    pub codec_tag: u32,
    pub profile: i32,
    pub level: i32,
    pub coded_format: i32,
    pub bits_per_raw_sample: i32,
    pub bit_rate: i64,
    pub width: i32,
    pub height: i32,
    pub sample_aspect_ratio: (i32, i32),
    pub avg_frame_rate: (i32, i32),
    pub sample_rate: i32,
    pub channels: i32,
    pub channel_layout: String,
    pub color_primaries: i32,
    pub color_transfer: i32,
    pub color_matrix: i32,
    pub color_range: i32,
    pub time_base: (i32, i32),
    pub start_time: i64,
    pub duration: i64,
    pub declared_frames: i64,
    pub disposition: i32,
    pub extradata: Vec<u8>,
    pub tags: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PacketFacts {
    pub pts: i64,
    pub dts: i64,
    pub duration: i64,
    pub flags: i32,
    pub payload: Vec<u8>,
    pub side_data: Vec<(i32, Vec<u8>)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameFacts {
    pub pts: Option<i64>,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub visible_sha256: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaSnapshot {
    pub file_bytes: Vec<u8>,
    pub whole_file_sha256: [u8; 32],
    pub format_start_time: i64,
    pub format_duration: i64,
    pub format_tags: BTreeMap<String, String>,
    pub streams: Vec<StreamFacts>,
    pub packets: Vec<Vec<PacketFacts>>,
    pub interleave: Vec<usize>,
    pub frames: Vec<FrameFacts>,
}

#[derive(Clone, Copy, Debug)]
pub struct ComparisonPolicy {
    /// Only `format.tags.encoder=LavfM.m.p` and
    /// `stream.tags.encoder=LavcM.m.p`, with unchanged M and m.
    pub allow_ffmpeg_patch_metadata: bool,
    /// Only the pinned H.264 VAAPI encoder-identifier SEI may vary by patch.
    pub allow_h264_encoder_patch_sei: bool,
    /// Opt in only after independently attesting the exact build identity.
    pub require_raw_packet_identity: bool,
    pub require_whole_file_identity: bool,
}

impl Default for ComparisonPolicy {
    fn default() -> Self {
        Self {
            allow_ffmpeg_patch_metadata: true,
            allow_h264_encoder_patch_sei: true,
            require_raw_packet_identity: false,
            require_whole_file_identity: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct MediaRegressionResult {
    pub media_differences: Vec<String>,
    pub raw_packet_differences: Vec<String>,
    pub decoded_differences: Vec<String>,
    pub structure_differences: Vec<String>,
    pub ordering_differences: Vec<String>,
    pub approved_volatile_differences: Vec<String>,
    pub approved_coded_metadata_differences: Vec<String>,
    approved_coded_byte_pairs: Vec<(Vec<u8>, Vec<u8>)>,
    pub whole_file_equal: bool,
    require_raw_packet_identity: bool,
    require_whole_file_identity: bool,
}

impl MediaRegressionResult {
    pub fn media_semantics_equal(&self) -> bool {
        self.coded_semantics_equal() && self.decoded_identity_equal()
    }
    pub fn raw_packet_identity_equal(&self) -> bool {
        self.raw_packet_differences.is_empty()
    }
    pub fn coded_semantics_equal(&self) -> bool {
        self.media_differences.is_empty()
    }
    pub fn decoded_identity_equal(&self) -> bool {
        self.decoded_differences.is_empty()
    }
    pub fn container_structure_equal(&self) -> bool {
        self.structure_differences.is_empty() && self.ordering_differences.is_empty()
    }
    pub fn passes(&self) -> bool {
        self.media_semantics_equal()
            && self.container_structure_equal()
            && (!self.require_raw_packet_identity || self.raw_packet_identity_equal())
            && (!self.require_whole_file_identity || self.whole_file_equal)
    }
    pub fn report(&self) -> String {
        let mut out = format!(
            "Media regression {}\nTier 1A raw packet identity: {}\nTier 1B coded semantic identity: {}\nTier 1C decoded identity: {}\nTier 2 structure: {}\nTier 3 whole-file SHA-256 bytes (build identity not attested here): {}\n",
            if self.passes() { "PASS" } else { "FAILED" },
            if self.raw_packet_identity_equal() {
                "PASS"
            } else {
                "FAIL"
            },
            if self.coded_semantics_equal() {
                "PASS"
            } else {
                "FAIL"
            },
            if self.decoded_identity_equal() {
                "PASS"
            } else {
                "FAIL"
            },
            if self.container_structure_equal() {
                "PASS"
            } else {
                "FAIL"
            },
            if self.whole_file_equal {
                "MATCH"
            } else {
                "DIFFERENT"
            }
        );
        for (heading, entries) in [
            ("Raw packet differences", &self.raw_packet_differences),
            ("Coded semantic differences", &self.media_differences),
            ("Decoded differences", &self.decoded_differences),
            ("Structural differences", &self.structure_differences),
            ("Container ordering differences", &self.ordering_differences),
            (
                "Approved volatile differences",
                &self.approved_volatile_differences,
            ),
            (
                "Approved coded-metadata differences",
                &self.approved_coded_metadata_differences,
            ),
        ] {
            for entry in entries {
                out.push_str(&format!("{heading}: {entry}\n"));
            }
        }
        out
    }
}

struct Format(*mut ffi::AVFormatContext);
impl Drop for Format {
    fn drop(&mut self) {
        unsafe {
            ffi::avformat_close_input(&mut self.0);
        }
    }
}
struct Packet(*mut ffi::AVPacket);
impl Drop for Packet {
    fn drop(&mut self) {
        unsafe {
            ffi::av_packet_free(&mut self.0);
        }
    }
}

pub fn inspect(path: &Path) -> Check<MediaSnapshot> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let whole_file_sha256 = sha256(&bytes)?;
    let native = CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| "input path contains NUL".to_string())?;
    let mut raw = ptr::null_mut();
    let rc = unsafe {
        ffi::avformat_open_input(&mut raw, native.as_ptr(), ptr::null(), ptr::null_mut())
    };
    if rc < 0 || raw.is_null() {
        return Err(format!("open {}: FFmpeg error {rc}", path.display()));
    }
    let format = Format(raw);
    let rc = unsafe { ffi::avformat_find_stream_info(format.0, ptr::null_mut()) };
    if rc < 0 {
        return Err(format!("probe {}: FFmpeg error {rc}", path.display()));
    }
    let ctx = unsafe { &*format.0 };
    if ctx.nb_streams == 0 || ctx.nb_streams > 128 || ctx.streams.is_null() {
        return Err(format!("unsupported stream table size {}", ctx.nb_streams));
    }
    let mut streams = Vec::with_capacity(ctx.nb_streams as usize);
    for i in 0..ctx.nb_streams as usize {
        let stream_ptr = unsafe { *ctx.streams.add(i) };
        if stream_ptr.is_null() {
            return Err(format!("null stream {i}"));
        }
        let stream = unsafe { &*stream_ptr };
        if stream.codecpar.is_null() {
            return Err(format!("null codec parameters for stream {i}"));
        }
        let par = unsafe { &*stream.codecpar };
        if par.extradata_size < 0 {
            return Err(format!("negative extradata size for stream {i}"));
        }
        let extradata = native_bytes(par.extradata, par.extradata_size as usize)?;
        streams.push(StreamFacts {
            index: stream.index,
            kind: par.codec_type as i32,
            codec: par.codec_id as i32,
            codec_tag: par.codec_tag,
            profile: par.profile,
            level: par.level,
            coded_format: par.format,
            bits_per_raw_sample: par.bits_per_raw_sample,
            bit_rate: par.bit_rate,
            width: par.width,
            height: par.height,
            sample_aspect_ratio: (
                stream.sample_aspect_ratio.num,
                stream.sample_aspect_ratio.den,
            ),
            avg_frame_rate: (stream.avg_frame_rate.num, stream.avg_frame_rate.den),
            sample_rate: par.sample_rate,
            channels: par.ch_layout.nb_channels,
            channel_layout: if par.codec_type == ffi::AVMediaType::AVMEDIA_TYPE_AUDIO {
                describe_channel_layout(&par.ch_layout)?
            } else {
                String::new()
            },
            color_primaries: par.color_primaries as i32,
            color_transfer: par.color_trc as i32,
            color_matrix: par.color_space as i32,
            color_range: par.color_range as i32,
            time_base: (stream.time_base.num, stream.time_base.den),
            start_time: stream.start_time,
            duration: stream.duration,
            declared_frames: stream.nb_frames,
            disposition: stream.disposition,
            extradata,
            tags: read_tags(stream.metadata)?,
        });
    }
    let mut packets = vec![Vec::new(); streams.len()];
    let mut interleave = Vec::new();
    let raw_packet = unsafe { ffi::av_packet_alloc() };
    if raw_packet.is_null() {
        return Err("av_packet_alloc failed".into());
    }
    let packet = Packet(raw_packet);
    loop {
        let rc = unsafe { ffi::av_read_frame(format.0, packet.0) };
        if rc == ffi::AVERROR_EOF {
            break;
        }
        if rc < 0 {
            return Err(format!("read packet: FFmpeg error {rc}"));
        }
        let p = unsafe { &*packet.0 };
        let index = usize::try_from(p.stream_index).map_err(|_| "negative packet stream index")?;
        if index >= packets.len() {
            return Err(format!("packet stream {index} is out of range"));
        }
        if p.size < 0 || p.side_data_elems < 0 || (p.side_data_elems > 0 && p.side_data.is_null()) {
            return Err(format!("invalid packet storage in stream {index}"));
        }
        let mut side_data = Vec::new();
        for i in 0..p.side_data_elems as usize {
            let item = unsafe { &*p.side_data.add(i) };
            side_data.push((item.type_ as i32, native_bytes(item.data, item.size)?));
        }
        packets[index].push(PacketFacts {
            pts: p.pts,
            dts: p.dts,
            duration: p.duration,
            flags: p.flags,
            payload: native_bytes(p.data, p.size as usize)?,
            side_data,
        });
        interleave.push(index);
        unsafe {
            ffi::av_packet_unref(packet.0);
        }
    }
    let format_tags = read_tags(ctx.metadata)?;
    let format_start_time = ctx.start_time;
    let format_duration = ctx.duration;
    drop(packet);
    drop(format);

    let mut decoder = Decoder::open(path).map_err(|e| format!("software decode: {e}"))?;
    let mut frames = Vec::new();
    while let Some(frame) = decoder
        .next_frame()
        .map_err(|e| format!("decode frame: {e}"))?
    {
        let desc = frame.desc();
        let (y, uv) = frame.host().planes(desc);
        // HostFrame is tightly packed: these slices contain only visible
        // width × height plane samples, never AVFrame stride padding.
        let mut visible = Vec::with_capacity(y.len() + uv.len());
        visible.extend_from_slice(y);
        visible.extend_from_slice(uv);
        frames.push(FrameFacts {
            pts: frame.pts(),
            width: desc.width,
            height: desc.height,
            format: desc.format,
            visible_sha256: sha256(&visible)?,
        });
    }
    Ok(MediaSnapshot {
        file_bytes: bytes,
        whole_file_sha256,
        format_start_time,
        format_duration,
        format_tags,
        streams,
        packets,
        interleave,
        frames,
    })
}

pub fn compare_files(
    reference: &Path,
    candidate: &Path,
    policy: ComparisonPolicy,
) -> Check<MediaRegressionResult> {
    let expected = inspect(reference)?;
    let actual = inspect(candidate)?;
    Ok(compare(&expected, &actual, policy))
}

fn native_bytes(data: *const u8, len: usize) -> Check<Vec<u8>> {
    if len == 0 {
        return Ok(Vec::new());
    }
    if data.is_null() {
        return Err("FFmpeg exposed null nonempty data".into());
    }
    Ok(unsafe { std::slice::from_raw_parts(data, len) }.to_vec())
}

fn read_tags(dict: *mut ffi::AVDictionary) -> Check<BTreeMap<String, String>> {
    let mut tags = BTreeMap::new();
    let mut entry = ptr::null();
    loop {
        entry = unsafe { ffi::av_dict_get(dict, c"".as_ptr(), entry, ffi::AV_DICT_IGNORE_SUFFIX) };
        if entry.is_null() {
            break;
        }
        let key = unsafe { CStr::from_ptr((*entry).key) }
            .to_str()
            .map_err(|e| format!("non-UTF8 metadata key: {e}"))?;
        let value = unsafe { CStr::from_ptr((*entry).value) }
            .to_str()
            .map_err(|e| format!("non-UTF8 metadata value: {e}"))?;
        if tags.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(format!("duplicate metadata key {key}"));
        }
    }
    Ok(tags)
}

fn describe_channel_layout(layout: &ffi::AVChannelLayout) -> Check<String> {
    let mut buffer = [0i8; 256];
    let needed =
        unsafe { ffi::av_channel_layout_describe(layout, buffer.as_mut_ptr(), buffer.len()) };
    if needed < 0 || needed as usize >= buffer.len() {
        return Err(format!(
            "audio channel layout description failed or truncated: {needed}"
        ));
    }
    Ok(unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_str()
        .map_err(|e| format!("non-UTF8 channel layout: {e}"))?
        .to_owned())
}

fn sha256(data: &[u8]) -> Check<[u8; 32]> {
    let ctx = unsafe { ffi::av_sha_alloc() };
    if ctx.is_null() {
        return Err("av_sha_alloc failed".into());
    }
    let mut digest = [0u8; 32];
    let rc = unsafe { ffi::av_sha_init(ctx, 256) };
    if rc >= 0 {
        unsafe {
            ffi::av_sha_update(ctx, data.as_ptr(), data.len());
        }
        unsafe {
            ffi::av_sha_final(ctx, digest.as_mut_ptr());
        }
    }
    unsafe {
        ffi::av_free(ctx.cast());
    }
    if rc < 0 {
        return Err(format!("av_sha_init failed: {rc}"));
    }
    Ok(digest)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn compare_tags(
    scope: &str,
    reference: &BTreeMap<String, String>,
    candidate: &BTreeMap<String, String>,
    policy: ComparisonPolicy,
    result: &mut MediaRegressionResult,
) {
    let keys: BTreeSet<_> = reference.keys().chain(candidate.keys()).collect();
    for key in keys {
        let before = reference.get(key);
        let after = candidate.get(key);
        if before == after {
            continue;
        }
        let approved = if policy.allow_ffmpeg_patch_metadata && key == "encoder" {
            let prefix = if scope == "format" { "Lavf" } else { "Lavc" };
            before
                .zip(after)
                .is_some_and(|(a, b)| same_ffmpeg_family_patch(a, b, prefix))
        } else {
            false
        };
        let difference = format!("{scope}.tags.{key}: {:?} -> {:?}", before, after);
        if approved {
            result
                .approved_volatile_differences
                .push(format!("{difference} (FFmpeg patch version)"));
        } else if !result.structure_differences.contains(&difference) {
            result.structure_differences.push(difference);
        }
    }
}

fn same_ffmpeg_family_patch(a: &str, b: &str, prefix: &str) -> bool {
    fn parts<'a>(value: &'a str, prefix: &str) -> Option<[&'a str; 3]> {
        let mut split = value.strip_prefix(prefix)?.split('.');
        let values = [split.next()?, split.next()?, split.next()?];
        if split.next().is_some()
            || values
                .iter()
                .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
        {
            return None;
        }
        Some(values)
    }
    match (parts(a, prefix), parts(b, prefix)) {
        (Some(x), Some(y)) => a.len() == b.len() && x[0] == y[0] && x[1] == y[1] && x[2] != y[2],
        _ => false,
    }
}

/// Supplementary byte-scope guard: after the full native semantic/structural
/// comparison, require every remaining file byte delta to fall inside an
/// explicitly approved, equal-length version tag. This is not the oracle's
/// sole comparison and never exempts packet bytes from Tier 1.
fn mask_approved_version_tags(
    reference: &MediaSnapshot,
    candidate: &MediaSnapshot,
    policy: ComparisonPolicy,
    approved_coded_byte_pairs: &[(Vec<u8>, Vec<u8>)],
) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut a = reference.file_bytes.clone();
    let mut b = candidate.file_bytes.clone();
    let mut values = Vec::new();
    if policy.allow_ffmpeg_patch_metadata {
        if let (Some(x), Some(y)) = (
            reference.format_tags.get("encoder"),
            candidate.format_tags.get("encoder"),
        ) {
            if same_ffmpeg_family_patch(x, y, "Lavf") {
                values.push((x.as_bytes(), y.as_bytes()));
            }
        }
        for (x, y) in reference.streams.iter().zip(&candidate.streams) {
            if let (Some(before), Some(after)) = (x.tags.get("encoder"), y.tags.get("encoder")) {
                if same_ffmpeg_family_patch(before, after, "Lavc") {
                    values.push((before.as_bytes(), after.as_bytes()));
                }
            }
        }
    }
    values.extend(
        approved_coded_byte_pairs
            .iter()
            .map(|(before, after)| (before.as_slice(), after.as_slice())),
    );
    for (before, after) in values {
        if a.len() < before.len() || b.len() < after.len() {
            return None;
        }
        let positions_a: Vec<_> = a
            .windows(before.len())
            .enumerate()
            .filter_map(|(i, bytes)| (bytes == before).then_some(i))
            .collect();
        let positions_b: Vec<_> = b
            .windows(after.len())
            .enumerate()
            .filter_map(|(i, bytes)| (bytes == after).then_some(i))
            .collect();
        // One parsed tag may authorize only one byte occurrence. Duplicates
        // could live in opaque atoms and must not be silently normalized.
        if positions_a.len() != 1 || positions_b.len() != 1 {
            return None;
        }
        a[positions_a[0]..positions_a[0] + before.len()].fill(0);
        b[positions_b[0]..positions_b[0] + after.len()].fill(0);
    }
    Some((a, b))
}

pub fn compare(
    reference: &MediaSnapshot,
    candidate: &MediaSnapshot,
    policy: ComparisonPolicy,
) -> MediaRegressionResult {
    let mut result = MediaRegressionResult {
        whole_file_equal: reference.whole_file_sha256 == candidate.whole_file_sha256,
        require_raw_packet_identity: policy.require_raw_packet_identity,
        require_whole_file_identity: policy.require_whole_file_identity,
        ..MediaRegressionResult::default()
    };
    macro_rules! media {
        ($name:expr, $a:expr, $b:expr) => {
            if $a != $b {
                result
                    .media_differences
                    .push(format!("{}: {:?} -> {:?}", $name, $a, $b));
            }
        };
    }
    macro_rules! structure {
        ($name:expr, $a:expr, $b:expr) => {
            if $a != $b {
                result
                    .structure_differences
                    .push(format!("{}: {:?} -> {:?}", $name, $a, $b));
            }
        };
    }
    media!(
        "stream count",
        reference.streams.len(),
        candidate.streams.len()
    );
    structure!(
        "stream count",
        reference.streams.len(),
        candidate.streams.len()
    );
    structure!(
        "format start time",
        reference.format_start_time,
        candidate.format_start_time
    );
    structure!(
        "format duration",
        reference.format_duration,
        candidate.format_duration
    );
    compare_tags(
        "format",
        &reference.format_tags,
        &candidate.format_tags,
        policy,
        &mut result,
    );
    for (i, (a, b)) in reference.streams.iter().zip(&candidate.streams).enumerate() {
        let scope = format!("stream {i}");
        media!(format!("{scope} codec"), a.codec, b.codec);
        media!(format!("{scope} kind"), a.kind, b.kind);
        media!(format!("{scope} profile"), a.profile, b.profile);
        media!(
            format!("{scope} coded format"),
            a.coded_format,
            b.coded_format
        );
        media!(
            format!("{scope} bit depth"),
            a.bits_per_raw_sample,
            b.bits_per_raw_sample
        );
        media!(
            format!("{scope} dimensions"),
            (a.width, a.height),
            (b.width, b.height)
        );
        media!(
            format!("{scope} audio layout"),
            (a.sample_rate, a.channels),
            (b.sample_rate, b.channels)
        );
        media!(
            format!("{scope} channel layout"),
            a.channel_layout,
            b.channel_layout
        );
        media!(
            format!("{scope} color primaries"),
            a.color_primaries,
            b.color_primaries
        );
        media!(
            format!("{scope} color transfer"),
            a.color_transfer,
            b.color_transfer
        );
        media!(
            format!("{scope} color matrix"),
            a.color_matrix,
            b.color_matrix
        );
        media!(format!("{scope} color range"), a.color_range, b.color_range);
        media!(format!("{scope} disposition"), a.disposition, b.disposition);
        media!(
            format!("{scope} language"),
            a.tags.get("language"),
            b.tags.get("language")
        );
        structure!(format!("{scope} identity"), a.index, b.index);
        structure!(
            format!("{scope} codec tag/level/bitrate"),
            (a.codec_tag, a.level, a.bit_rate),
            (b.codec_tag, b.level, b.bit_rate)
        );
        structure!(
            format!("{scope} aspect/rate"),
            (a.sample_aspect_ratio, a.avg_frame_rate),
            (b.sample_aspect_ratio, b.avg_frame_rate)
        );
        structure!(format!("{scope} time base"), a.time_base, b.time_base);
        structure!(
            format!("{scope} start/duration/frames"),
            (a.start_time, a.duration, a.declared_frames),
            (b.start_time, b.duration, b.declared_frames)
        );
        structure!(
            format!("{scope} extradata SHA-256"),
            hex(&sha256(&a.extradata).unwrap()),
            hex(&sha256(&b.extradata).unwrap())
        );
        compare_tags(&scope, &a.tags, &b.tags, policy, &mut result);
    }
    media!(
        "packet stream count",
        reference.packets.len(),
        candidate.packets.len()
    );
    for (stream, (a, b)) in reference.packets.iter().zip(&candidate.packets).enumerate() {
        media!(format!("stream {stream} packet count"), a.len(), b.len());
        for (index, (x, y)) in a.iter().zip(b).enumerate() {
            let scope = format!("stream {stream} packet {index}");
            media!(
                format!("{scope} PTS/DTS/duration"),
                (x.pts, x.dts, x.duration),
                (y.pts, y.dts, y.duration)
            );
            media!(format!("{scope} flags"), x.flags, y.flags);
            media!(
                format!("{scope} payload length"),
                x.payload.len(),
                y.payload.len()
            );
            if x.payload != y.payload {
                let raw_difference = format!(
                    "{scope} payload mismatch: expected sha256 {}, actual sha256 {}",
                    hex(&sha256(&x.payload).unwrap()),
                    hex(&sha256(&y.payload).unwrap())
                );
                result.raw_packet_differences.push(raw_difference.clone());
                let h264 = reference
                    .streams
                    .get(stream)
                    .zip(candidate.streams.get(stream))
                    .is_some_and(|(before, after)| {
                        before.codec == ffi::AVCodecID::AV_CODEC_ID_H264 as i32
                            && after.codec == before.codec
                            && before.extradata == after.extradata
                    });
                let approval = if h264 && policy.allow_h264_encoder_patch_sei {
                    reference.streams.get(stream).and_then(|facts| {
                        avcc_length_size(&facts.extradata).ok().map(|length_size| {
                            compare_avcc_packets(&x.payload, &y.payload, length_size)
                        })
                    })
                } else {
                    None
                };
                match approval {
                    Some(Ok(changes)) => {
                        for change in changes {
                            result.approved_coded_metadata_differences.push(format!(
                                "{scope} NAL {} SEI {} user_data_unregistered UUID {}: {} -> {} (encoder patch version)",
                                change.nal_index,
                                change.sei_index,
                                hex(&change.uuid),
                                change.reference_text,
                                change.candidate_text
                            ));
                            result
                                .approved_coded_byte_pairs
                                .push((change.reference_token, change.candidate_token));
                        }
                    }
                    Some(Err(reason)) => result.media_differences.push(format!(
                        "{raw_difference}; H.264 semantic comparison: {reason}"
                    )),
                    None => result.media_differences.push(raw_difference),
                }
            }
            if x.side_data != y.side_data {
                result
                    .media_differences
                    .push(format!("{scope} side data mismatch"));
            }
        }
    }
    media!(
        "decoded frame count",
        reference.frames.len(),
        candidate.frames.len()
    );
    for (index, (a, b)) in reference.frames.iter().zip(&candidate.frames).enumerate() {
        media!(format!("frame {index} PTS"), a.pts, b.pts);
        media!(
            format!("frame {index} dimensions"),
            (a.width, a.height),
            (b.width, b.height)
        );
        media!(format!("frame {index} pixel format"), a.format, b.format);
        if a.visible_sha256 != b.visible_sha256 {
            result.decoded_differences.push(format!(
                "frame {index} visible pixel SHA-256: {} -> {}",
                hex(&a.visible_sha256),
                hex(&b.visible_sha256)
            ));
        }
    }
    if reference.interleave != candidate.interleave {
        result.ordering_differences.push(
            "cross-stream packet interleave changed; per-stream media comparison retained".into(),
        );
    }
    if !result.whole_file_equal
        && result.media_semantics_equal()
        && result.container_structure_equal()
    {
        let only_approved_bytes = mask_approved_version_tags(
            reference,
            candidate,
            policy,
            &result.approved_coded_byte_pairs,
        )
        .is_some_and(|(a, b)| a == b);
        if !only_approved_bytes {
            result
                .structure_differences
                .push("whole-file bytes differ outside approved version tag values or encoder SEI patch token".into());
        }
    }
    result
}
