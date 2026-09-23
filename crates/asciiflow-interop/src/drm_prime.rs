use asciiflow_core::{Error, PixelFormat, Result};
use asciiflow_media::{VaapiDecodedFrame, VaapiEncoderFrame};
use ffmpeg_sys_next as ffi;
use std::{
    os::fd::{FromRawFd, OwnedFd},
    ptr::NonNull,
    time::{Duration, Instant},
};

use asciiflow_vulkan::{ExternalImageAccess, ExternalPlaneImage, ExternalPlaneKind};

const DRM_FORMAT_R8: u32 = u32::from_le_bytes(*b"R8  ");
const DRM_FORMAT_GR88: u32 = u32::from_le_bytes(*b"GR88");
const DRM_FORMAT_R16: u32 = u32::from_le_bytes(*b"R16 ");
const DRM_FORMAT_GR32: u32 = u32::from_le_bytes(*b"GR32");

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DrmObject {
    pub fd: i32,
    pub size: u64,
    pub format_modifier: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DrmPlane {
    pub object_index: usize,
    pub offset: u64,
    pub pitch: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DrmLayer {
    pub format: u32,
    pub planes: Vec<DrmPlane>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DrmPrimeFrameDesc {
    pub width: u32,
    pub height: u32,
    pub objects: Vec<DrmObject>,
    pub layers: Vec<DrmLayer>,
}

pub struct DrmPrimeMapping<T = VaapiDecodedFrame> {
    source: Option<T>,
    mapped: NonNull<ffi::AVFrame>,
    descriptor: DrmPrimeFrameDesc,
    map_wall: Duration,
    access: ExternalImageAccess,
}

impl DrmPrimeMapping<VaapiDecodedFrame> {
    pub fn map_direct_read(source: VaapiDecodedFrame) -> Result<Self> {
        let source_pointer = unsafe { source.as_raw_ptr() };
        let width = source.desc().width;
        let height = source.desc().height;
        Self::map(
            source,
            source_pointer,
            width,
            height,
            (ffi::AV_HWFRAME_MAP_READ as i32) | (ffi::AV_HWFRAME_MAP_DIRECT as i32),
            ExternalImageAccess::Read,
            "direct VAAPI to DRM PRIME read mapping",
        )
    }

    pub fn pts(&self) -> Option<i64> {
        self.source
            .as_ref()
            .expect("DRM mapping source already taken")
            .pts()
    }
}

impl DrmPrimeMapping<VaapiEncoderFrame> {
    pub fn map_direct_write(source: VaapiEncoderFrame) -> Result<Self> {
        let source_pointer = unsafe { source.as_raw_ptr() };
        let width = source.desc().width;
        let height = source.desc().height;
        Self::map(
            source,
            source_pointer,
            width,
            height,
            (ffi::AV_HWFRAME_MAP_WRITE as i32)
                | (ffi::AV_HWFRAME_MAP_OVERWRITE as i32)
                | (ffi::AV_HWFRAME_MAP_DIRECT as i32),
            ExternalImageAccess::Write,
            "direct VAAPI encoder surface to DRM PRIME write mapping",
        )
    }

    pub fn into_source(mut self) -> VaapiEncoderFrame {
        self.source
            .take()
            .expect("DRM mapping source already taken")
    }
}

impl<T> DrmPrimeMapping<T> {
    fn map(
        source: T,
        source_pointer: *const ffi::AVFrame,
        width: u32,
        height: u32,
        flags: i32,
        access: ExternalImageAccess,
        operation: &str,
    ) -> Result<Self> {
        let mapped = NonNull::new(unsafe { ffi::av_frame_alloc() })
            .ok_or_else(|| Error::Media("failed to allocate DRM PRIME mapped frame".into()))?;
        let mut guard = MappedFrameGuard(Some(mapped));
        unsafe {
            (*mapped.as_ptr()).format = ffi::AVPixelFormat::AV_PIX_FMT_DRM_PRIME as i32;
        }
        let started = Instant::now();
        let result = unsafe { ffi::av_hwframe_map(mapped.as_ptr(), source_pointer, flags) };
        let map_wall = started.elapsed();
        if result < 0 {
            return Err(Error::Media(format!(
                "{operation} failed ({result}); explicit interop does not fall back to a copy"
            )));
        }
        let native = unsafe { &*mapped.as_ptr() };
        if native.format != ffi::AVPixelFormat::AV_PIX_FMT_DRM_PRIME as i32 {
            return Err(Error::Media(format!(
                "direct hardware mapping returned pixel format {}; expected DRM PRIME",
                native.format
            )));
        }
        let descriptor_ptr = native.data[0].cast::<ffi::AVDRMFrameDescriptor>();
        let descriptor = snapshot_descriptor(width, height, descriptor_ptr)?;
        Ok(Self {
            source: Some(source),
            mapped: guard.take(),
            descriptor,
            map_wall,
            access,
        })
    }

    pub fn descriptor(&self) -> &DrmPrimeFrameDesc {
        &self.descriptor
    }

    pub fn map_wall(&self) -> Duration {
        self.map_wall
    }

    pub fn duplicate_external_planes(&self) -> Result<[ExternalPlaneImage; 2]> {
        self.duplicate_planes(PixelFormat::Nv12)
    }

    /// Duplicate the observed two-layer P010 DRM PRIME layout for Vulkan input.
    pub fn duplicate_external_p010_planes(&self) -> Result<[ExternalPlaneImage; 2]> {
        if self.access != ExternalImageAccess::Read {
            return Err(Error::UnsupportedFrame(
                "P010 DRM PRIME output import is not qualified".into(),
            ));
        }
        self.duplicate_planes(PixelFormat::P010Le)
    }

    fn duplicate_planes(&self, format: PixelFormat) -> Result<[ExternalPlaneImage; 2]> {
        let (y_format, uv_format, y_kind, uv_kind) = match format {
            PixelFormat::Nv12 => (
                DRM_FORMAT_R8,
                DRM_FORMAT_GR88,
                ExternalPlaneKind::Y,
                ExternalPlaneKind::Uv,
            ),
            PixelFormat::P010Le => (
                DRM_FORMAT_R16,
                DRM_FORMAT_GR32,
                ExternalPlaneKind::P010Y,
                ExternalPlaneKind::P010Uv,
            ),
        };
        let desc = &self.descriptor;
        if desc.width == 0 || desc.height == 0 || desc.width % 2 != 0 || desc.height % 2 != 0 {
            return Err(Error::UnsupportedFrame(
                "VAAPI/Vulkan interop requires non-zero even-sized frames".into(),
            ));
        }
        if desc.objects.len() != 1
            || desc.layers.len() != 2
            || desc.layers[0].format != y_format
            || desc.layers[1].format != uv_format
            || desc.layers.iter().any(|layer| layer.planes.len() != 1)
            || desc
                .layers
                .iter()
                .any(|layer| layer.planes[0].object_index != 0)
        {
            return Err(Error::UnsupportedFrame(format!(
                "unsupported DRM PRIME layout: objects={}, layers={:?}; expected one object with {} and {} single-plane layers",
                desc.objects.len(),
                desc.layers
                    .iter()
                    .map(|layer| fourcc_name(layer.format))
                    .collect::<Vec<_>>(),
                fourcc_name(y_format),
                fourcc_name(uv_format),
            )));
        }
        let object = &desc.objects[0];
        if object.format_modifier == u64::MAX {
            return Err(Error::UnsupportedFrame(
                "DRM PRIME mapping did not provide a usable format modifier".into(),
            ));
        }
        let y = &desc.layers[0].planes[0];
        let uv = &desc.layers[1].planes[0];
        let row_bytes = u64::from(desc.width) * format.bytes_per_sample() as u64;
        if y.pitch < row_bytes || uv.pitch < row_bytes {
            return Err(Error::UnsupportedFrame(format!(
                "DRM PRIME pitch is too small: Y={}, UV={}, row bytes={row_bytes}",
                y.pitch, uv.pitch
            )));
        }
        require_plane_in_object("Y", y, desc.height, row_bytes, object.size)?;
        require_plane_in_object("UV", uv, desc.height / 2, row_bytes, object.size)?;
        Ok([
            ExternalPlaneImage {
                fd: duplicate_fd(object.fd)?,
                object_size: object.size,
                modifier: object.format_modifier,
                offset: y.offset,
                row_pitch: y.pitch,
                width: desc.width,
                height: desc.height,
                kind: y_kind,
                access: self.access,
            },
            ExternalPlaneImage {
                fd: duplicate_fd(object.fd)?,
                object_size: object.size,
                modifier: object.format_modifier,
                offset: uv.offset,
                row_pitch: uv.pitch,
                width: desc.width / 2,
                height: desc.height / 2,
                kind: uv_kind,
                access: self.access,
            },
        ])
    }
}

impl<T> Drop for DrmPrimeMapping<T> {
    fn drop(&mut self) {
        let mut pointer = self.mapped.as_ptr();
        unsafe { ffi::av_frame_free(&mut pointer) };
    }
}

unsafe impl<T: Send> Send for DrmPrimeMapping<T> {}

fn snapshot_descriptor(
    width: u32,
    height: u32,
    descriptor: *const ffi::AVDRMFrameDescriptor,
) -> Result<DrmPrimeFrameDesc> {
    let descriptor = unsafe { descriptor.as_ref() }
        .ok_or_else(|| Error::Media("DRM PRIME frame contains no descriptor".into()))?;
    let object_count = bounded_count("object", descriptor.nb_objects, descriptor.objects.len())?;
    let layer_count = bounded_count("layer", descriptor.nb_layers, descriptor.layers.len())?;
    let objects = descriptor.objects[..object_count]
        .iter()
        .map(|object| {
            if object.fd < 0 || object.size == 0 {
                return Err(Error::Media(format!(
                    "DRM PRIME object has invalid fd {} or size {}",
                    object.fd, object.size
                )));
            }
            Ok(DrmObject {
                fd: object.fd,
                size: object.size as u64,
                format_modifier: object.format_modifier,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let layers = descriptor.layers[..layer_count]
        .iter()
        .map(|layer| {
            let plane_count = bounded_count("plane", layer.nb_planes, layer.planes.len())?;
            let planes = layer.planes[..plane_count]
                .iter()
                .map(|plane| {
                    let object_index = usize::try_from(plane.object_index).map_err(|_| {
                        Error::Media(format!(
                            "DRM PRIME plane has negative object index {}",
                            plane.object_index
                        ))
                    })?;
                    if object_index >= objects.len() || plane.pitch == 0 {
                        return Err(Error::Media(format!(
                            "DRM PRIME plane references object {object_index} of {} or has zero pitch",
                            objects.len()
                        )));
                    }
                    let offset = u64::try_from(plane.offset).map_err(|_| {
                        Error::Media(format!(
                            "DRM PRIME plane has negative offset {}",
                            plane.offset
                        ))
                    })?;
                    let pitch = u64::try_from(plane.pitch).map_err(|_| {
                        Error::Media(format!(
                            "DRM PRIME plane has negative pitch {}",
                            plane.pitch
                        ))
                    })?;
                    Ok(DrmPlane {
                        object_index,
                        offset,
                        pitch,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(DrmLayer {
                format: layer.format,
                planes,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(DrmPrimeFrameDesc {
        width,
        height,
        objects,
        layers,
    })
}

fn bounded_count(kind: &str, value: i32, capacity: usize) -> Result<usize> {
    let count = usize::try_from(value)
        .map_err(|_| Error::Media(format!("DRM PRIME {kind} count is negative: {value}")))?;
    if count == 0 || count > capacity {
        Err(Error::Media(format!(
            "DRM PRIME {kind} count {count} is outside 1..={capacity}"
        )))
    } else {
        Ok(count)
    }
}

pub fn fourcc_name(value: u32) -> String {
    value
        .to_le_bytes()
        .into_iter()
        .map(|byte| {
            if byte.is_ascii_graphic() {
                byte as char
            } else {
                '.'
            }
        })
        .collect()
}

fn duplicate_fd(fd: i32) -> Result<OwnedFd> {
    let duplicate = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
    if duplicate < 0 {
        Err(Error::Media(format!(
            "failed to duplicate DMA-BUF fd: {}",
            std::io::Error::last_os_error()
        )))
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(duplicate) })
    }
}

fn require_plane_in_object(
    name: &str,
    plane: &DrmPlane,
    rows: u32,
    row_bytes: u64,
    object_size: u64,
) -> Result<()> {
    let end = u64::from(rows.saturating_sub(1))
        .checked_mul(plane.pitch)
        .and_then(|last_row| plane.offset.checked_add(last_row))
        .and_then(|last_row| last_row.checked_add(row_bytes))
        .ok_or_else(|| Error::Media(format!("DRM PRIME {name} plane range overflow")))?;
    if end > object_size {
        Err(Error::Media(format!(
            "DRM PRIME {name} plane ends at {end}, beyond object size {object_size}"
        )))
    } else {
        Ok(())
    }
}

struct MappedFrameGuard(Option<NonNull<ffi::AVFrame>>);

impl MappedFrameGuard {
    fn take(&mut self) -> NonNull<ffi::AVFrame> {
        self.0.take().expect("mapped frame guard already empty")
    }
}

impl Drop for MappedFrameGuard {
    fn drop(&mut self) {
        if let Some(frame) = self.0 {
            let mut pointer = frame.as_ptr();
            unsafe { ffi::av_frame_free(&mut pointer) };
        }
    }
}
