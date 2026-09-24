use super::{codec::ffmpeg_error, ffi, frame::Frame, hwdevice::HardwareDevice};
#[cfg(feature = "p010-output-diagnostic")]
use asciiflow_core::ColorSpace;
use asciiflow_core::{Error, FrameDesc, HostFrame, PixelFormat, Result, VideoFrame};
#[cfg(feature = "p010-output-diagnostic")]
use std::path::Path;
use std::{ptr, ptr::NonNull};

pub(crate) struct HardwareFramesPool {
    reference: NonNull<ffi::AVBufferRef>,
}

impl HardwareFramesPool {
    pub(crate) fn vaapi_nv12(device: &HardwareDevice, width: u32, height: u32) -> Result<Self> {
        Self::vaapi(device, width, height, ffi::AVPixelFormat::AV_PIX_FMT_NV12)
    }

    #[cfg(feature = "p010-output-diagnostic")]
    pub(crate) fn vaapi_p010(device: &HardwareDevice, width: u32, height: u32) -> Result<Self> {
        Self::vaapi(device, width, height, ffi::AVPixelFormat::AV_PIX_FMT_P010LE)
    }

    fn vaapi(
        device: &HardwareDevice,
        width: u32,
        height: u32,
        sw_format: ffi::AVPixelFormat,
    ) -> Result<Self> {
        let reference = NonNull::new(unsafe { ffi::av_hwframe_ctx_alloc(device.as_ptr()) })
            .ok_or_else(|| Error::Media("failed to allocate VAAPI frames context".into()))?;
        let mut guard = BufferGuard(Some(reference));
        let context = unsafe { (*reference.as_ptr()).data.cast::<ffi::AVHWFramesContext>() };
        if context.is_null() {
            return Err(Error::Media(
                "VAAPI frames reference contains a null context".into(),
            ));
        }
        unsafe {
            (*context).format = ffi::AVPixelFormat::AV_PIX_FMT_VAAPI;
            (*context).sw_format = sw_format;
            (*context).width = width as i32;
            (*context).height = height as i32;
            // VAAPI supports a dynamic pool. This keeps the pool bounded by
            // actual encoder retention instead of copying an arbitrary sample
            // constant into the stream lifetime.
            (*context).initial_pool_size = 0;
        }
        let result = unsafe { ffi::av_hwframe_ctx_init(reference.as_ptr()) };
        if result < 0 {
            return Err(ffmpeg_error(
                "failed to initialize VAAPI frames pool",
                result,
            ));
        }
        Ok(Self {
            reference: guard.take(),
        })
    }

    pub(crate) fn as_ptr(&self) -> *mut ffi::AVBufferRef {
        self.reference.as_ptr()
    }

    pub(crate) fn try_clone_ref(&self) -> Result<*mut ffi::AVBufferRef> {
        let reference = unsafe { ffi::av_buffer_ref(self.reference.as_ptr()) };
        if reference.is_null() {
            Err(Error::Media(
                "failed to retain VAAPI frames-pool reference".into(),
            ))
        } else {
            Ok(reference)
        }
    }

    fn try_clone(&self) -> Result<Self> {
        NonNull::new(unsafe { ffi::av_buffer_ref(self.reference.as_ptr()) })
            .map(|reference| Self { reference })
            .ok_or_else(|| Error::Media("failed to retain VAAPI frames-pool reference".into()))
    }

    pub(crate) fn supports_upload_nv12(&self) -> Result<bool> {
        supports_format(
            self.reference.as_ptr(),
            ffi::AVHWFrameTransferDirection::AV_HWFRAME_TRANSFER_DIRECTION_TO,
            ffi::AVPixelFormat::AV_PIX_FMT_NV12,
        )
    }
}

impl Drop for HardwareFramesPool {
    fn drop(&mut self) {
        let mut reference = self.reference.as_ptr();
        unsafe { ffi::av_buffer_unref(&mut reference) };
    }
}

unsafe impl Send for HardwareFramesPool {}

/// Encoder-style P010 surfaces for opt-in output interop qualification only.
/// This pool never creates or exposes an encoder context.
#[cfg(feature = "p010-output-diagnostic")]
pub struct VaapiDiagnosticP010Pool {
    _device: HardwareDevice,
    frames: VaapiEncoderFrames,
}

#[cfg(feature = "p010-output-diagnostic")]
impl VaapiDiagnosticP010Pool {
    pub fn new(device_path: &Path, width: u32, height: u32) -> Result<Self> {
        let desc = FrameDesc::host_p010_le(width, height, ColorSpace::default())?;
        let device = HardwareDevice::vaapi(Some(device_path))?;
        let pool = HardwareFramesPool::vaapi_p010(&device, width, height)?;
        if !supports_format(
            pool.as_ptr(),
            ffi::AVHWFrameTransferDirection::AV_HWFRAME_TRANSFER_DIRECTION_TO,
            ffi::AVPixelFormat::AV_PIX_FMT_P010LE,
        )? || !supports_download_p010(pool.as_ptr())?
        {
            return Err(Error::UnsupportedFrame(
                "diagnostic VAAPI P010 pool cannot upload and download P010LE".into(),
            ));
        }
        Ok(Self {
            frames: VaapiEncoderFrames::from_pool(&pool, desc)?,
            _device: device,
        })
    }

    pub fn acquire(&self, pts: i64) -> Result<VaapiEncoderFrame> {
        self.frames.acquire(pts)
    }

    pub fn output_frames(&self) -> Result<VaapiEncoderFrames> {
        self.frames.clone_for_diagnostic()
    }
}

pub(crate) struct HardwareFrame {
    frame: Frame,
}

impl HardwareFrame {
    pub(crate) fn new() -> Result<Self> {
        Ok(Self {
            frame: Frame::new()?,
        })
    }

    pub(crate) fn allocate(&mut self, pool: &HardwareFramesPool) -> Result<()> {
        self.frame.unref();
        let result =
            unsafe { ffi::av_hwframe_get_buffer(pool.as_ptr(), self.frame.as_mut_ptr(), 0) };
        if result < 0 {
            return Err(ffmpeg_error("failed to allocate VAAPI frame", result));
        }
        let native = unsafe { &*self.frame.as_mut_ptr() };
        if native.format != ffi::AVPixelFormat::AV_PIX_FMT_VAAPI as i32
            || native.hw_frames_ctx.is_null()
        {
            return Err(Error::Media(
                "VAAPI pool returned a non-hardware frame".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut ffi::AVFrame {
        self.frame.as_mut_ptr()
    }

    fn as_ptr(&self) -> *const ffi::AVFrame {
        self.frame.as_ptr()
    }
}

/// A retained handle to a VAAPI frames context, normally configured on an encoder.
///
/// Clones share the native pool. Encoder-owned handles yield frames accepted by
/// that encoder; the opt-in P010 diagnostic pool has no encoder at all.
pub struct VaapiEncoderFrames {
    pool: HardwareFramesPool,
    desc: FrameDesc,
}

impl VaapiEncoderFrames {
    pub(crate) fn from_pool(pool: &HardwareFramesPool, desc: FrameDesc) -> Result<Self> {
        Ok(Self {
            pool: pool.try_clone()?,
            desc,
        })
    }

    pub fn acquire(&self, pts: i64) -> Result<VaapiEncoderFrame> {
        let mut hardware = HardwareFrame::new()?;
        hardware.allocate(&self.pool)?;
        let native = unsafe { &mut *hardware.as_mut_ptr() };
        native.pts = pts;
        native.color_range = ffi::AVColorRange::AVCOL_RANGE_MPEG;
        native.colorspace = ffi::AVColorSpace::AVCOL_SPC_BT709;
        native.color_primaries = ffi::AVColorPrimaries::AVCOL_PRI_BT709;
        native.color_trc = ffi::AVColorTransferCharacteristic::AVCOL_TRC_BT709;
        native.chroma_location = ffi::AVChromaLocation::AVCHROMA_LOC_LEFT;
        Ok(VaapiEncoderFrame {
            hardware,
            desc: self.desc.clone(),
        })
    }

    pub fn clone_for_diagnostic(&self) -> Result<Self> {
        Ok(Self {
            pool: self.pool.try_clone()?,
            desc: self.desc.clone(),
        })
    }
}

unsafe impl Send for VaapiEncoderFrames {}

/// One VAAPI output surface, normally encoder-compatible.
///
/// The value owns its `AVFrame` reference and may be moved to the interop
/// worker. It must remain alive until Vulkan has returned foreign ownership.
/// Encoder-owned surfaces may then be consumed by `Encoder::encode_hardware_frame`;
/// diagnostic surfaces may instead be downloaded for pixel qualification.
pub struct VaapiEncoderFrame {
    hardware: HardwareFrame,
    desc: FrameDesc,
}

impl VaapiEncoderFrame {
    pub fn desc(&self) -> &FrameDesc {
        &self.desc
    }

    pub fn pts(&self) -> i64 {
        unsafe { (*self.hardware.as_ptr()).pts }
    }

    /// Returns the native frame for the narrowly-scoped interop layer.
    ///
    /// # Safety
    ///
    /// The pointer is borrowed from `self`, must not be freed or mutated, and
    /// must not be used after `self` is dropped.
    pub unsafe fn as_raw_ptr(&self) -> *const ffi::AVFrame {
        self.hardware.as_ptr()
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut ffi::AVFrame {
        self.hardware.as_mut_ptr()
    }

    pub(crate) fn belongs_to(&self, pool: &HardwareFramesPool) -> bool {
        let frame_ref = unsafe { (*self.hardware.as_ptr()).hw_frames_ctx };
        !frame_ref.is_null() && unsafe { (*frame_ref).data == (*pool.as_ptr()).data }
    }

    /// Diagnostic upload used to compare the staged and external-image paths
    /// before either surface enters an encoder.
    pub fn upload_nv12(&mut self, frame: &VideoFrame) -> Result<()> {
        self.upload_host(frame, PixelFormat::Nv12)
    }

    #[cfg(feature = "p010-output-diagnostic")]
    pub fn upload_p010(&mut self, frame: &VideoFrame) -> Result<()> {
        self.upload_host(frame, PixelFormat::P010Le)
    }

    fn upload_host(&mut self, frame: &VideoFrame, expected: PixelFormat) -> Result<()> {
        if self.desc.format != expected {
            return Err(Error::UnsupportedFrame(format!(
                "diagnostic VAAPI upload expected {expected:?}, surface is {:?}",
                self.desc.format
            )));
        }
        if frame.desc() != &self.desc {
            return Err(Error::Media(
                "encoder-surface upload received a different frame descriptor".into(),
            ));
        }
        let mut host = Frame::new()?;
        unsafe {
            (*host.as_mut_ptr()).format = av_pixel_format(expected) as i32;
            (*host.as_mut_ptr()).width = self.desc.width as i32;
            (*host.as_mut_ptr()).height = self.desc.height as i32;
        }
        let allocated = unsafe { ffi::av_frame_get_buffer(host.as_mut_ptr(), 32) };
        if allocated < 0 {
            return Err(ffmpeg_error(
                "failed to allocate staged encoder upload frame",
                allocated,
            ));
        }
        let native = unsafe { &mut *host.as_mut_ptr() };
        let (source_y, source_uv) = frame.host().planes(frame.desc());
        let width = self.desc.y_stride();
        let height = self.desc.height as usize;
        unsafe {
            for row in 0..height {
                std::ptr::copy_nonoverlapping(
                    source_y.as_ptr().add(row * width),
                    native.data[0].add(row * native.linesize[0] as usize),
                    width,
                );
            }
            for row in 0..height / 2 {
                std::ptr::copy_nonoverlapping(
                    source_uv.as_ptr().add(row * width),
                    native.data[1].add(row * native.linesize[1] as usize),
                    width,
                );
            }
            native.pts = frame.pts().unwrap_or(self.pts());
        }
        let uploaded = unsafe {
            ffi::av_hwframe_transfer_data(self.hardware.as_mut_ptr(), host.as_mut_ptr(), 0)
        };
        if uploaded < 0 {
            return Err(ffmpeg_error(
                "failed to upload staged frame into VAAPI surface",
                uploaded,
            ));
        }
        unsafe { (*self.hardware.as_mut_ptr()).pts = native.pts };
        Ok(())
    }

    /// Diagnostic-only readback used to prove pre-encode pixel parity.
    pub fn download_nv12(&self) -> Result<VideoFrame> {
        self.download_as(PixelFormat::Nv12)
    }

    #[cfg(feature = "p010-output-diagnostic")]
    pub fn download_p010(&self) -> Result<VideoFrame> {
        self.download_as(PixelFormat::P010Le)
    }

    fn download_as(&self, expected: PixelFormat) -> Result<VideoFrame> {
        if self.desc.format != expected {
            return Err(Error::UnsupportedFrame(format!(
                "diagnostic VAAPI download expected {expected:?}, surface is {:?}",
                self.desc.format
            )));
        }
        let mut host = Frame::new()?;
        unsafe {
            (*host.as_mut_ptr()).format = av_pixel_format(expected) as i32;
        }
        let result =
            unsafe { ffi::av_hwframe_transfer_data(host.as_mut_ptr(), self.hardware.as_ptr(), 0) };
        if result < 0 {
            return Err(ffmpeg_error(
                "failed to download VAAPI encoder surface for diagnostic parity",
                result,
            ));
        }
        let native = unsafe { &*host.as_ptr() };
        if native.format != av_pixel_format(expected) as i32 {
            return Err(Error::UnsupportedFrame(format!(
                "VAAPI surface diagnostic produced pixel format {}; expected {expected:?}",
                native.format,
            )));
        }
        let width = self.desc.y_stride();
        let height = self.desc.height as usize;
        if native.linesize[0] < width as i32 || native.linesize[1] < width as i32 {
            return Err(Error::Media(
                "VAAPI surface diagnostic returned an undersized stride".into(),
            ));
        }
        let mut storage = HostFrame::new_zeroed(&self.desc);
        let (y, uv) = storage.planes_mut(&self.desc);
        unsafe {
            for row in 0..height {
                std::ptr::copy_nonoverlapping(
                    native.data[0].add(row * native.linesize[0] as usize),
                    y.as_mut_ptr().add(row * width),
                    width,
                );
            }
            for row in 0..height / 2 {
                std::ptr::copy_nonoverlapping(
                    native.data[1].add(row * native.linesize[1] as usize),
                    uv.as_mut_ptr().add(row * width),
                    width,
                );
            }
        }
        VideoFrame::new_host(self.desc.clone(), Some(self.pts()), storage)
    }
}

unsafe impl Send for VaapiEncoderFrame {}

fn av_pixel_format(format: PixelFormat) -> ffi::AVPixelFormat {
    match format {
        PixelFormat::Nv12 => ffi::AVPixelFormat::AV_PIX_FMT_NV12,
        PixelFormat::P010Le => ffi::AVPixelFormat::AV_PIX_FMT_P010LE,
    }
}

pub(crate) fn supports_download_nv12(frames: *mut ffi::AVBufferRef) -> Result<bool> {
    supports_format(
        frames,
        ffi::AVHWFrameTransferDirection::AV_HWFRAME_TRANSFER_DIRECTION_FROM,
        ffi::AVPixelFormat::AV_PIX_FMT_NV12,
    )
}

pub(crate) fn supports_download_p010(frames: *mut ffi::AVBufferRef) -> Result<bool> {
    supports_format(
        frames,
        ffi::AVHWFrameTransferDirection::AV_HWFRAME_TRANSFER_DIRECTION_FROM,
        ffi::AVPixelFormat::AV_PIX_FMT_P010LE,
    )
}

fn supports_format(
    frames: *mut ffi::AVBufferRef,
    direction: ffi::AVHWFrameTransferDirection,
    expected: ffi::AVPixelFormat,
) -> Result<bool> {
    let mut formats = ptr::null_mut();
    let result =
        unsafe { ffi::av_hwframe_transfer_get_formats(frames, direction, &mut formats, 0) };
    if result < 0 {
        return Err(ffmpeg_error(
            "failed to query VAAPI transfer formats",
            result,
        ));
    }
    if formats.is_null() {
        return Ok(false);
    }
    let mut found = false;
    let mut cursor = formats;
    unsafe {
        while *cursor != ffi::AVPixelFormat::AV_PIX_FMT_NONE {
            found |= *cursor == expected;
            cursor = cursor.add(1);
        }
        ffi::av_free(formats.cast());
    }
    Ok(found)
}

struct BufferGuard(Option<NonNull<ffi::AVBufferRef>>);
impl BufferGuard {
    fn take(&mut self) -> NonNull<ffi::AVBufferRef> {
        self.0.take().expect("buffer guard already empty")
    }
}
impl Drop for BufferGuard {
    fn drop(&mut self) {
        if let Some(reference) = self.0 {
            let mut pointer = reference.as_ptr();
            unsafe { ffi::av_buffer_unref(&mut pointer) };
        }
    }
}
