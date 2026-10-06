//! Opt-in, measurement-build-only evidence at the actual encoder boundary.
//! Readback is diagnostic: it never replaces or modifies the submitted surface.

use super::{codec::check, ffi, frame::Frame};
use asciiflow_core::{Error, Result};
use std::{
    ffi::CStr,
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    ptr,
};

pub(super) struct EncoderCapture {
    directory: PathBuf,
    metadata: BufWriter<File>,
    pixels: BufWriter<File>,
    index: u64,
}

fn create_file(path: &Path) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(capture_io)
}

fn capture_io(error: std::io::Error) -> Error {
    Error::Media(format!("encoder capture I/O: {error}"))
}

impl EncoderCapture {
    pub(super) fn from_environment(codec: &ffi::AVCodecContext) -> Result<Option<Self>> {
        let Some(directory) = std::env::var_os("ASCIIFLOW_ENCODER_CAPTURE_DIRECTORY") else {
            return Ok(None);
        };
        if codec.codec_id != ffi::AVCodecID::AV_CODEC_ID_H264
            || codec.pix_fmt != ffi::AVPixelFormat::AV_PIX_FMT_VAAPI
        {
            return Err(Error::Media(
                "encoder capture requires H.264 VAAPI, not a fallback".into(),
            ));
        }
        let directory = PathBuf::from(directory);
        std::fs::create_dir(&directory).map_err(capture_io)?;
        let metadata = BufWriter::new(create_file(&directory.join("frames.jsonl"))?);
        let pixels = BufWriter::new(create_file(&directory.join("frames.nv12"))?);
        let capture = Self {
            directory,
            metadata,
            pixels,
            index: 0,
        };
        capture.context("before", codec)?;
        Ok(Some(capture))
    }

    pub(super) fn context(&self, phase: &str, codec: &ffi::AVCodecContext) -> Result<()> {
        write_context(&self.directory, phase, codec)
    }

    pub(super) fn frame(&mut self, source: *const ffi::AVFrame) -> Result<()> {
        capture_frame(self, source)
    }
}

pub(super) fn write_context(
    directory: &Path,
    phase: &str,
    codec: &ffi::AVCodecContext,
) -> Result<()> {
    let mut output = create_file(&directory.join(format!("context-{phase}.json")))?;
    writeln!(
            output,
            "{{\"codec_id\":{},\"profile\":{},\"level\":{},\"width\":{},\"height\":{},\"pix_fmt\":{},\"time_base\":[{},{}],\"framerate\":[{},{}],\"sample_aspect_ratio\":[{},{}],\"bit_rate\":{},\"rc_max_rate\":{},\"rc_min_rate\":{},\"rc_buffer_size\":{},\"gop_size\":{},\"max_b_frames\":{},\"refs\":{},\"flags\":{},\"flags2\":{},\"global_quality\":{},\"compression_level\":{},\"qmin\":{},\"qmax\":{},\"qcompress\":{},\"qblur\":{},\"slices\":{},\"color_primaries\":{},\"color_trc\":{},\"colorspace\":{},\"color_range\":{},\"chroma_sample_location\":{}}}",
            codec.codec_id as i32,
            codec.profile,
            codec.level,
            codec.width,
            codec.height,
            codec.pix_fmt as i32,
            codec.time_base.num,
            codec.time_base.den,
            codec.framerate.num,
            codec.framerate.den,
            codec.sample_aspect_ratio.num,
            codec.sample_aspect_ratio.den,
            codec.bit_rate,
            codec.rc_max_rate,
            codec.rc_min_rate,
            codec.rc_buffer_size,
            codec.gop_size,
            codec.max_b_frames,
            codec.refs,
            codec.flags,
            codec.flags2,
            codec.global_quality,
            codec.compression_level,
            codec.qmin,
            codec.qmax,
            codec.qcompress,
            codec.qblur,
            codec.slices,
            codec.color_primaries as i32,
            codec.color_trc as i32,
            codec.colorspace as i32,
            codec.color_range as i32,
            codec.chroma_sample_location as i32
        ).map_err(capture_io)?;
    // libav's public serialization includes the encoder's private AVOptions;
    // opaque FFmpeg private struct layout is deliberately never inspected.
    let mut serialized = ptr::null_mut();
    let result = unsafe {
        ffi::av_opt_serialize(
            (codec as *const ffi::AVCodecContext).cast_mut().cast(),
            0,
            ffi::AV_OPT_SERIALIZE_SEARCH_CHILDREN,
            &mut serialized,
            b'=' as _,
            b';' as _,
        )
    };
    let options = if result >= 0 && !serialized.is_null() {
        unsafe { CStr::from_ptr(serialized) }.to_bytes().to_vec()
    } else {
        Vec::new()
    };
    unsafe { ffi::av_free(serialized.cast()) };
    check(result, "serialize encoder configuration for capture")?;
    if options.is_empty() {
        return Err(Error::Media("encoder AVOption capture was empty".into()));
    }
    create_file(&directory.join(format!("avoptions-{phase}.txt")))?
        .write_all(&options)
        .map_err(capture_io)?;
    Ok(())
}

fn capture_frame(capture: &mut EncoderCapture, source: *const ffi::AVFrame) -> Result<()> {
    if source.is_null() {
        return Err(Error::Media("encoder capture received a null frame".into()));
    }
    let native = unsafe { &*source };
    let (width, height) = dimensions(native.width, native.height)?;
    if native.format != ffi::AVPixelFormat::AV_PIX_FMT_VAAPI as i32 {
        return Err(Error::Media(
            "encoder capture expected an actual VAAPI surface".into(),
        ));
    }
    let mut host = Frame::new()?;
    unsafe { (*host.as_mut_ptr()).format = ffi::AVPixelFormat::AV_PIX_FMT_NV12 as i32 };
    check(
        unsafe { ffi::av_hwframe_transfer_data(host.as_mut_ptr(), source, 0) },
        "download final encoder surface for diagnostic capture",
    )?;
    let downloaded = unsafe { &*host.as_ptr() };
    if downloaded.format != ffi::AVPixelFormat::AV_PIX_FMT_NV12 as i32
        || downloaded.width != native.width
        || downloaded.height != native.height
    {
        return Err(Error::Media(
            "encoder capture download changed format/geometry".into(),
        ));
    }
    let y = plane(downloaded, 0, width, height)?;
    let uv = plane(downloaded, 1, width, height / 2)?;
    capture.pixels.write_all(&y).map_err(capture_io)?;
    capture.pixels.write_all(&uv).map_err(capture_io)?;
    writeln!(
            capture.metadata,
            "{{\"index\":{},\"format\":{},\"software_format\":{},\"width\":{},\"height\":{},\"pts\":{},\"duration\":{},\"time_base\":[{},{}],\"sample_aspect_ratio\":[{},{}],\"flags\":{},\"pict_type\":{},\"color_primaries\":{},\"color_trc\":{},\"colorspace\":{},\"color_range\":{},\"chroma_location\":{},\"y_sha256\":\"{}\",\"uv_sha256\":\"{}\"}}",
            capture.index,
            native.format,
            downloaded.format,
            width,
            height,
            native.pts,
            native.duration,
            native.time_base.num,
            native.time_base.den,
            native.sample_aspect_ratio.num,
            native.sample_aspect_ratio.den,
            native.flags,
            native.pict_type as i32,
            native.color_primaries as i32,
            native.color_trc as i32,
            native.colorspace as i32,
            native.color_range as i32,
            native.chroma_location as i32,
            sha256(&y)?,
            sha256(&uv)?
        ).map_err(capture_io)?;
    capture.index += 1;
    // Evidence errors must be visible before submitting the surface.
    capture.metadata.flush().map_err(capture_io)?;
    capture.pixels.flush().map_err(capture_io)?;
    Ok(())
}

fn dimensions(width: i32, height: i32) -> Result<(usize, usize)> {
    if width <= 0
        || height <= 0
        || width > 16384
        || height > 16384
        || width % 2 != 0
        || height % 2 != 0
    {
        return Err(Error::Media(
            "unsafe NV12 encoder capture dimensions".into(),
        ));
    }
    Ok((width as usize, height as usize))
}

fn plane(frame: &ffi::AVFrame, index: usize, width: usize, rows: usize) -> Result<Vec<u8>> {
    if frame.data[index].is_null() || frame.linesize[index] < width as i32 {
        return Err(Error::Media("invalid encoder capture plane/stride".into()));
    }
    let mut packed = Vec::with_capacity(width * rows);
    for row in 0..rows {
        // FFmpeg owns the successful download's plane allocations. Copy only
        // visible bytes while its RAII Frame is alive; ignore driver padding.
        let bytes = unsafe {
            std::slice::from_raw_parts(
                frame.data[index].add(row * frame.linesize[index] as usize),
                width,
            )
        };
        packed.extend_from_slice(bytes);
    }
    Ok(packed)
}

pub(super) fn sha256(bytes: &[u8]) -> Result<String> {
    let state = unsafe { ffi::av_sha_alloc() };
    if state.is_null() {
        return Err(Error::Media("allocate encoder capture SHA-256".into()));
    }
    let result = unsafe { ffi::av_sha_init(state, 256) };
    let mut hash = [0u8; 32];
    if result >= 0 {
        unsafe {
            ffi::av_sha_update(state, bytes.as_ptr(), bytes.len());
            ffi::av_sha_final(state, hash.as_mut_ptr());
        }
    }
    unsafe { ffi::av_free(state.cast()) };
    check(result, "initialize encoder capture SHA-256")?;
    Ok(hash.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_rejects_unsafe_geometry_and_bad_planes() {
        for pair in [(0, 1080), (-2, 1080), (1920, 1079), (16386, 1080)] {
            assert!(dimensions(pair.0, pair.1).is_err());
        }
        assert_eq!(dimensions(1920, 1080).unwrap(), (1920, 1080));
        let frame = Frame::new().unwrap();
        assert!(plane(unsafe { &*frame.as_ptr() }, 0, 1920, 1080).is_err());
    }
    #[test]
    fn capture_hash_has_independent_known_answer() {
        assert_eq!(
            sha256(b"abc").unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
