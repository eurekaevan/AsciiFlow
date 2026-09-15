use ffmpeg_sys_next as ffi;
use std::{
    ffi::{CStr, CString},
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Workspace(pub PathBuf);
impl Workspace {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "asciiflow-audio-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    pub fn output(&self) -> PathBuf {
        self.0.join("output.mp4")
    }
    pub fn assert_no_staging(&self) {
        for entry in fs::read_dir(&self.0).unwrap() {
            assert!(
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains("asciiflow-part")
            );
        }
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/media")
        .join(name)
}
pub fn command(input: &Path, output: &Path, policy: &str) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_asciiflow"));
    cmd.arg(input).arg(output).args([
        "--backend",
        "cpu",
        "--decode",
        "software",
        "--encode",
        "software",
        "--audio",
        policy,
        "--width",
        "16",
        "--no-progress",
    ]);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd
}
pub struct Process(Option<Child>);
impl Process {
    pub fn start(cmd: &mut Command) -> Self {
        Self(Some(cmd.spawn().unwrap()))
    }
    pub fn pid(&self) -> u32 {
        self.0.as_ref().unwrap().id()
    }
    pub fn finish(mut self) -> Output {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if self.0.as_mut().unwrap().try_wait().unwrap().is_some() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "audio conversion deadlocked or exceeded 30 seconds"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        self.0.take().unwrap().wait_with_output().unwrap()
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
pub fn run(input: &str, policy: &str) -> (Workspace, Output) {
    let ws = Workspace::new();
    let result = Process::start(&mut command(&fixture(input), &ws.output(), policy)).finish();
    ws.assert_no_staging();
    (ws, result)
}
pub fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[derive(Debug, PartialEq)]
pub struct Packet {
    pub data: Vec<u8>,
    pub pts: i64,
    pub dts: i64,
    pub duration: i64,
}
#[derive(Debug)]
pub struct Stream {
    pub audio: bool,
    pub video: bool,
    pub codec: String,
    pub rate: i32,
    pub channels: i32,
    pub language: Option<String>,
    pub default: bool,
    pub num: i32,
    pub den: i32,
    pub packets: Vec<Packet>,
    pub duration: i64,
}
struct Input(*mut ffi::AVFormatContext);
impl Drop for Input {
    fn drop(&mut self) {
        unsafe {
            ffi::avformat_close_input(&mut self.0);
        }
    }
}
struct NativePacket(*mut ffi::AVPacket);
impl Drop for NativePacket {
    fn drop(&mut self) {
        unsafe {
            ffi::av_packet_free(&mut self.0);
        }
    }
}
pub fn inspect(path: &Path) -> Vec<Stream> {
    unsafe {
        let name = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        let mut input = Input(std::ptr::null_mut());
        assert!(
            ffi::avformat_open_input(
                &mut input.0,
                name.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut()
            ) >= 0
        );
        assert!(ffi::avformat_find_stream_info(input.0, std::ptr::null_mut()) >= 0);
        let mut streams: Vec<_> = (0..(*input.0).nb_streams as usize)
            .map(|index| {
                let stream = &**(*input.0).streams.add(index);
                let params = &*stream.codecpar;
                let lang =
                    ffi::av_dict_get(stream.metadata, c"language".as_ptr(), std::ptr::null(), 0);
                Stream {
                    audio: params.codec_type == ffi::AVMediaType::AVMEDIA_TYPE_AUDIO,
                    video: params.codec_type == ffi::AVMediaType::AVMEDIA_TYPE_VIDEO,
                    codec: CStr::from_ptr(ffi::avcodec_get_name(params.codec_id))
                        .to_string_lossy()
                        .into_owned(),
                    rate: params.sample_rate,
                    channels: params.ch_layout.nb_channels,
                    language: (!lang.is_null())
                        .then(|| CStr::from_ptr((*lang).value).to_string_lossy().into_owned()),
                    default: stream.disposition & ffi::AV_DISPOSITION_DEFAULT != 0,
                    num: stream.time_base.num,
                    den: stream.time_base.den,
                    packets: Vec::new(),
                    duration: stream.duration,
                }
            })
            .collect();
        let packet = NativePacket(ffi::av_packet_alloc());
        assert!(!packet.0.is_null());
        loop {
            let result = ffi::av_read_frame(input.0, packet.0);
            if result == ffi::AVERROR_EOF {
                break;
            }
            assert!(result >= 0, "packet read failed: {result}");
            let p = &*packet.0;
            let data = if p.size == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(p.data, p.size as usize).to_vec()
            };
            streams[p.stream_index as usize].packets.push(Packet {
                data,
                pts: p.pts,
                dts: p.dts,
                duration: p.duration,
            });
            ffi::av_packet_unref(packet.0);
        }
        streams
    }
}
pub fn audio(streams: &[Stream]) -> Vec<&Stream> {
    streams.iter().filter(|s| s.audio).collect()
}

/// Decode AAC only in the test oracle, never in the production pipeline.
pub fn assert_audio_decodes(path: &Path) {
    struct Codec(*mut ffi::AVCodecContext);
    impl Drop for Codec {
        fn drop(&mut self) {
            unsafe {
                ffi::avcodec_free_context(&mut self.0);
            }
        }
    }
    struct Frame(*mut ffi::AVFrame);
    impl Drop for Frame {
        fn drop(&mut self) {
            unsafe {
                ffi::av_frame_free(&mut self.0);
            }
        }
    }
    unsafe {
        let name = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        let mut input = Input(std::ptr::null_mut());
        assert!(
            ffi::avformat_open_input(
                &mut input.0,
                name.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut()
            ) >= 0
        );
        assert!(ffi::avformat_find_stream_info(input.0, std::ptr::null_mut()) >= 0);
        let index = ffi::av_find_best_stream(
            input.0,
            ffi::AVMediaType::AVMEDIA_TYPE_AUDIO,
            -1,
            -1,
            std::ptr::null_mut(),
            0,
        );
        assert!(index >= 0);
        let params = (**(*input.0).streams.add(index as usize)).codecpar;
        let decoder = ffi::avcodec_find_decoder((*params).codec_id);
        assert!(!decoder.is_null());
        let codec = Codec(ffi::avcodec_alloc_context3(decoder));
        assert!(!codec.0.is_null());
        assert!(ffi::avcodec_parameters_to_context(codec.0, params) >= 0);
        assert!(ffi::avcodec_open2(codec.0, decoder, std::ptr::null_mut()) >= 0);
        let frame = Frame(ffi::av_frame_alloc());
        let packet = NativePacket(ffi::av_packet_alloc());
        assert!(!frame.0.is_null() && !packet.0.is_null());
        let mut samples = 0;
        loop {
            let read = ffi::av_read_frame(input.0, packet.0);
            let eof = read == ffi::AVERROR_EOF;
            assert!(read >= 0 || eof);
            if eof || (*packet.0).stream_index == index {
                assert!(
                    ffi::avcodec_send_packet(
                        codec.0,
                        if eof { std::ptr::null() } else { packet.0 }
                    ) >= 0
                );
                loop {
                    let result = ffi::avcodec_receive_frame(codec.0, frame.0);
                    if result == ffi::AVERROR(libc::EAGAIN) || result == ffi::AVERROR_EOF {
                        break;
                    }
                    assert!(result >= 0, "audio decode failed: {result}");
                    samples += (*frame.0).nb_samples;
                    ffi::av_frame_unref(frame.0);
                }
            }
            ffi::av_packet_unref(packet.0);
            if eof {
                break;
            }
        }
        assert!(samples > 0);
    }
}
pub fn seconds(value: i64, stream: &Stream) -> f64 {
    value as f64 * stream.num as f64 / stream.den as f64
}
pub fn same_audio(input: &Stream, output: &Stream) {
    assert_eq!(
        (
            &input.codec,
            input.rate,
            input.channels,
            &input.language,
            input.default
        ),
        (
            &output.codec,
            output.rate,
            output.channels,
            &output.language,
            output.default
        )
    );
    assert_eq!(input.packets.len(), output.packets.len());
    let same_time = |x: i64, y: i64| {
        let difference = (i128::from(x) * i128::from(input.num) * i128::from(output.den)
            - i128::from(y) * i128::from(output.num) * i128::from(input.den))
        .abs();
        assert!(
            difference <= i128::from(output.num) * i128::from(input.den),
            "media timestamp changed"
        );
    };
    if input.duration != ffi::AV_NOPTS_VALUE && output.duration != ffi::AV_NOPTS_VALUE {
        same_time(input.duration, output.duration);
    }
    for (a, b) in input.packets.iter().zip(&output.packets) {
        assert_eq!(
            a.data, b.data,
            "compressed payload changed or crossed audio tracks"
        );
        for (x, y) in [(a.pts, b.pts), (a.dts, b.dts), (a.duration, b.duration)] {
            same_time(x, y);
        }
    }
}
