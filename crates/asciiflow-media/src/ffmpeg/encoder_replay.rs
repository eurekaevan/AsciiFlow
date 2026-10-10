//! Retained-input encoder isolation. No decoder, renderer, audio or muxer participates.
use super::*;
use crate::ffmpeg::encoder_capture::{sha256, write_context};
use serde_json::Value;
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
};

const WIDTH: usize = 1920;
const HEIGHT: usize = 1080;
const FRAMES: usize = 300;
const Y_BYTES: usize = WIDTH * HEIGHT;
const FRAME_BYTES: usize = Y_BYTES * 3 / 2;

fn integer(value: &Value, key: &str) -> i64 {
    value[key]
        .as_i64()
        .unwrap_or_else(|| panic!("missing integer {key}"))
}

fn rational(value: &Value, key: &str) -> ffi::AVRational {
    let values = value[key].as_array().expect("missing rational");
    assert_eq!(values.len(), 2);
    ffi::AVRational {
        num: values[0].as_i64().unwrap().try_into().unwrap(),
        den: values[1].as_i64().unwrap().try_into().unwrap(),
    }
}

// Only the scalar enum values this SDR capture can legitimately contain are
// accepted. Arbitrary recorded integers are never transmuted into Rust enums.
macro_rules! recorded_enum {
    ($value:expr, $key:literal, $ty:ident, [$($variant:ident),+]) => {{
        let number = integer($value, $key);
        match number {
            $(n if n == ffi::$ty::$variant as i64 => ffi::$ty::$variant,)+
            _ => panic!("unsupported recorded {}: {number}", $key),
        }
    }};
}

fn restore_context(codec: &mut ffi::AVCodecContext, recorded: &Value) {
    macro_rules! fields {
        ($($field:ident),+) => {$(codec.$field = integer(recorded, stringify!($field)).try_into().unwrap();)+};
    }
    fields!(
        profile,
        level,
        width,
        height,
        bit_rate,
        rc_max_rate,
        rc_min_rate,
        rc_buffer_size,
        gop_size,
        max_b_frames,
        refs,
        flags,
        flags2,
        global_quality,
        compression_level,
        qmin,
        qmax,
        slices
    );
    codec.qcompress = recorded["qcompress"].as_f64().unwrap() as f32;
    codec.qblur = recorded["qblur"].as_f64().unwrap() as f32;
    codec.time_base = rational(recorded, "time_base");
    codec.framerate = rational(recorded, "framerate");
    codec.sample_aspect_ratio = rational(recorded, "sample_aspect_ratio");
    assert_eq!(
        integer(recorded, "codec_id"),
        ffi::AVCodecID::AV_CODEC_ID_H264 as i64
    );
    assert_eq!(
        integer(recorded, "pix_fmt"),
        ffi::AVPixelFormat::AV_PIX_FMT_VAAPI as i64
    );
    codec.codec_id = ffi::AVCodecID::AV_CODEC_ID_H264;
    codec.codec_type = ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
    codec.pix_fmt = ffi::AVPixelFormat::AV_PIX_FMT_VAAPI;
    codec.color_primaries = recorded_enum!(
        recorded,
        "color_primaries",
        AVColorPrimaries,
        [AVCOL_PRI_BT709]
    );
    codec.color_trc = recorded_enum!(
        recorded,
        "color_trc",
        AVColorTransferCharacteristic,
        [AVCOL_TRC_BT709]
    );
    codec.colorspace = recorded_enum!(recorded, "colorspace", AVColorSpace, [AVCOL_SPC_BT709]);
    codec.color_range = recorded_enum!(recorded, "color_range", AVColorRange, [AVCOL_RANGE_MPEG]);
    codec.chroma_sample_location = recorded_enum!(
        recorded,
        "chroma_sample_location",
        AVChromaLocation,
        [AVCHROMA_LOC_LEFT]
    );
    assert_eq!((codec.width, codec.height), (WIDTH as i32, HEIGHT as i32));
    assert_eq!((codec.time_base.num, codec.time_base.den), (1, 50));
    assert_eq!((codec.framerate.num, codec.framerate.den), (50, 1));
    assert_eq!((codec.gop_size, codec.max_b_frames), (250, 0));
    assert_ne!(codec.flags & ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32, 0);
}

fn restore_frame(frame: &mut ffi::AVFrame, recorded: &Value) {
    frame.pts = integer(recorded, "pts");
    frame.duration = integer(recorded, "duration");
    frame.time_base = rational(recorded, "time_base");
    frame.sample_aspect_ratio = rational(recorded, "sample_aspect_ratio");
    frame.flags = integer(recorded, "flags").try_into().unwrap();
    frame.pict_type = recorded_enum!(
        recorded,
        "pict_type",
        AVPictureType,
        [
            AV_PICTURE_TYPE_NONE,
            AV_PICTURE_TYPE_I,
            AV_PICTURE_TYPE_P,
            AV_PICTURE_TYPE_B
        ]
    );
    frame.color_primaries = recorded_enum!(
        recorded,
        "color_primaries",
        AVColorPrimaries,
        [AVCOL_PRI_UNSPECIFIED, AVCOL_PRI_BT709]
    );
    frame.color_trc = recorded_enum!(
        recorded,
        "color_trc",
        AVColorTransferCharacteristic,
        [AVCOL_TRC_UNSPECIFIED, AVCOL_TRC_BT709]
    );
    frame.colorspace = recorded_enum!(
        recorded,
        "colorspace",
        AVColorSpace,
        [AVCOL_SPC_UNSPECIFIED, AVCOL_SPC_BT709, AVCOL_SPC_RGB]
    );
    frame.color_range = recorded_enum!(
        recorded,
        "color_range",
        AVColorRange,
        [AVCOL_RANGE_UNSPECIFIED, AVCOL_RANGE_MPEG]
    );
    frame.chroma_location = recorded_enum!(
        recorded,
        "chroma_location",
        AVChromaLocation,
        [AVCHROMA_LOC_UNSPECIFIED, AVCHROMA_LOC_LEFT]
    );
}

fn annex_b(bytes: &[u8]) {
    assert!(
        bytes.starts_with(&[0, 0, 1]) || bytes.starts_with(&[0, 0, 0, 1]),
        "encoder did not produce Annex B"
    );
}

fn drain(
    codec: *mut ffi::AVCodecContext,
    packet: &mut Packet,
    output: &mut File,
    count: &mut usize,
) -> i32 {
    loop {
        let result = unsafe { ffi::avcodec_receive_packet(codec, packet.as_mut_ptr()) };
        if again(result) || result == ffi::AVERROR_EOF {
            return result;
        }
        check(result, "receive encoder-only packet").unwrap();
        let native = unsafe { &*packet.as_mut_ptr() };
        assert!(native.size > 0 && !native.data.is_null());
        let bytes = unsafe { std::slice::from_raw_parts(native.data, native.size as usize) };
        annex_b(bytes);
        output.write_all(bytes).unwrap();
        *count += 1;
        packet.unref();
    }
}

#[test]
#[ignore = "requires retained native encoder NV12 capture and explicit VAAPI device/output"]
fn retained_nv12_h264_vaapi_encoder_only() {
    let input = PathBuf::from(
        std::env::var_os("ASCIIFLOW_ENCODER_REPLAY_INPUT")
            .expect("explicit capture directory required"),
    );
    let output_dir = PathBuf::from(
        std::env::var_os("ASCIIFLOW_ENCODER_REPLAY_OUTPUT")
            .expect("explicit new output directory required"),
    );
    let device_path = PathBuf::from(
        std::env::var_os("ASCIIFLOW_ENCODER_REPLAY_DEVICE")
            .expect("explicit VAAPI render node required"),
    );
    let recorded: Value =
        serde_json::from_reader(File::open(input.join("context-before.json")).unwrap()).unwrap();
    let metadata: Vec<Value> = BufReader::new(File::open(input.join("frames.jsonl")).unwrap())
        .lines()
        .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
        .collect();
    assert_eq!(metadata.len(), FRAMES);
    let mut pixels = File::open(input.join("frames.nv12")).unwrap();
    assert_eq!(
        pixels.metadata().unwrap().len(),
        (FRAME_BYTES * FRAMES) as u64
    );
    std::fs::create_dir(&output_dir).unwrap();
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output_dir.join("encoder-only.h264"))
        .unwrap();
    let device = HardwareDevice::vaapi(Some(&device_path)).unwrap();
    let pool = HardwareFramesPool::vaapi_nv12(&device, WIDTH as u32, HEIGHT as u32).unwrap();
    let encoder = unsafe { ffi::avcodec_find_encoder_by_name(c"h264_vaapi".as_ptr()) };
    assert!(!encoder.is_null(), "h264_vaapi unavailable");
    let codec = NonNull::new(unsafe { ffi::avcodec_alloc_context3(encoder) }).unwrap();
    let _guard = CodecGuard(Some(codec));
    unsafe {
        restore_context(&mut *codec.as_ptr(), &recorded);
        (*codec.as_ptr()).hw_frames_ctx = pool.try_clone_ref().unwrap();
    }
    write_context(&output_dir, "before", unsafe { &*codec.as_ptr() }).unwrap();
    compare_context(&input, &output_dir, "before");
    let mut options = ptr::null_mut();
    for (key, value) in vaapi_codec_options(&VideoCodec::H264) {
        let key = CString::new(*key).unwrap();
        let value = CString::new(*value).unwrap();
        check(
            unsafe { ffi::av_dict_set(&mut options, key.as_ptr(), value.as_ptr(), 0) },
            "encoder-only option",
        )
        .unwrap();
    }
    let opened = unsafe { ffi::avcodec_open2(codec.as_ptr(), encoder, &mut options) };
    let unused = unsafe { ffi::av_dict_count(options) };
    unsafe { ffi::av_dict_free(&mut options) };
    check(opened, "open encoder-only H264 VAAPI").unwrap();
    assert_eq!(unused, 0);
    write_context(&output_dir, "after", unsafe { &*codec.as_ptr() }).unwrap();
    compare_context(&input, &output_dir, "after");
    let native = unsafe { &*codec.as_ptr() };
    assert!(native.extradata_size > 0 && !native.extradata.is_null());
    let headers =
        unsafe { std::slice::from_raw_parts(native.extradata, native.extradata_size as usize) };
    annex_b(headers);
    output.write_all(headers).unwrap();
    let mut bytes = vec![0; FRAME_BYTES];
    let mut packet = Packet::new().unwrap();
    let mut packets = 0;
    for (index, recorded) in metadata.iter().enumerate() {
        assert_eq!(integer(recorded, "index"), index as i64);
        assert_eq!(
            integer(recorded, "format"),
            ffi::AVPixelFormat::AV_PIX_FMT_VAAPI as i64
        );
        assert_eq!(
            integer(recorded, "software_format"),
            ffi::AVPixelFormat::AV_PIX_FMT_NV12 as i64
        );
        assert_eq!(
            (integer(recorded, "width"), integer(recorded, "height")),
            (WIDTH as i64, HEIGHT as i64)
        );
        pixels.read_exact(&mut bytes).unwrap();
        assert_eq!(
            sha256(&bytes[..Y_BYTES]).unwrap(),
            recorded["y_sha256"].as_str().unwrap()
        );
        assert_eq!(
            sha256(&bytes[Y_BYTES..]).unwrap(),
            recorded["uv_sha256"].as_str().unwrap()
        );
        let mut host = Frame::new().unwrap();
        let native = unsafe { &mut *host.as_mut_ptr() };
        native.format = ffi::AVPixelFormat::AV_PIX_FMT_NV12 as i32;
        native.width = WIDTH as i32;
        native.height = HEIGHT as i32;
        check(
            unsafe { ffi::av_frame_get_buffer(host.as_mut_ptr(), 32) },
            "allocate retained NV12 upload",
        )
        .unwrap();
        let native = unsafe { &*host.as_ptr() };
        for (plane, rows, offset) in [(0, HEIGHT, 0), (1, HEIGHT / 2, Y_BYTES)] {
            assert!(native.linesize[plane] >= WIDTH as i32 && !native.data[plane].is_null());
            for row in 0..rows {
                unsafe {
                    ptr::copy_nonoverlapping(
                        bytes[offset + row * WIDTH..].as_ptr(),
                        native.data[plane].add(row * native.linesize[plane] as usize),
                        WIDTH,
                    )
                };
            }
        }
        let mut surface = Frame::new().unwrap();
        check(
            unsafe { ffi::av_hwframe_get_buffer(pool.as_ptr(), surface.as_mut_ptr(), 0) },
            "acquire replay encoder surface",
        )
        .unwrap();
        check(
            unsafe { ffi::av_hwframe_transfer_data(surface.as_mut_ptr(), host.as_ptr(), 0) },
            "upload retained NV12",
        )
        .unwrap();
        let uploaded = unsafe { &*surface.as_ptr() };
        assert_eq!(uploaded.format, ffi::AVPixelFormat::AV_PIX_FMT_VAAPI as i32);
        assert_eq!(
            (uploaded.width, uploaded.height),
            (WIDTH as i32, HEIGHT as i32)
        );
        restore_frame(unsafe { &mut *surface.as_mut_ptr() }, recorded);
        let sent = unsafe { ffi::avcodec_send_frame(codec.as_ptr(), surface.as_ptr()) };
        if again(sent) {
            let previous = packets;
            drain(codec.as_ptr(), &mut packet, &mut output, &mut packets);
            assert!(packets > previous, "encoder send/drain stalled");
            check(
                unsafe { ffi::avcodec_send_frame(codec.as_ptr(), surface.as_ptr()) },
                "retry retained encoder frame",
            )
            .unwrap();
        } else {
            check(sent, "submit retained encoder frame").unwrap();
        }
        assert_ne!(
            drain(codec.as_ptr(), &mut packet, &mut output, &mut packets),
            ffi::AVERROR_EOF
        );
    }
    check(
        unsafe { ffi::avcodec_send_frame(codec.as_ptr(), ptr::null()) },
        "flush encoder-only stream",
    )
    .unwrap();
    assert_eq!(
        drain(codec.as_ptr(), &mut packet, &mut output, &mut packets),
        ffi::AVERROR_EOF
    );
    assert_eq!(
        packets, FRAMES,
        "expected one access-unit packet per submitted frame"
    );
    assert_eq!(pixels.read(&mut [0]).unwrap(), 0);
    output.sync_all().unwrap();
}

fn compare_context(input: &Path, output: &Path, phase: &str) {
    let name = format!("context-{phase}.json");
    let source: Value = serde_json::from_reader(File::open(input.join(&name)).unwrap()).unwrap();
    let replay: Value = serde_json::from_reader(File::open(output.join(&name)).unwrap()).unwrap();
    assert_eq!(
        source, replay,
        "recorded encoder context differs {phase} open"
    );
    let name = format!("avoptions-{phase}.txt");
    assert_eq!(
        std::fs::read(input.join(&name)).unwrap(),
        std::fs::read(output.join(&name)).unwrap(),
        "recorded AVOptions differ {phase} open"
    );
}
