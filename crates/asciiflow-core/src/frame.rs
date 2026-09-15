use crate::{Error, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelFormat {
    Nv12,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryDomain {
    Host,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorMatrix {
    Bt601,
    Bt709,
    Unspecified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorRange {
    Limited,
    Full,
    Unspecified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorPrimaries {
    Bt709,
    Unspecified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferCharacteristic {
    Bt709,
    Unspecified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChromaLocation {
    Left,
    Center,
    Unspecified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorSpace {
    pub matrix: ColorMatrix,
    pub range: ColorRange,
    pub primaries: ColorPrimaries,
    pub transfer: TransferCharacteristic,
    pub chroma_location: ChromaLocation,
}

impl Default for ColorSpace {
    fn default() -> Self {
        Self {
            matrix: ColorMatrix::Bt709,
            range: ColorRange::Limited,
            primaries: ColorPrimaries::Bt709,
            transfer: TransferCharacteristic::Bt709,
            chroma_location: ChromaLocation::Left,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rational {
    pub numerator: i32,
    pub denominator: i32,
}

impl Rational {
    pub fn new(numerator: i32, denominator: i32) -> Result<Self> {
        if numerator <= 0 || denominator <= 0 {
            return Err(Error::InvalidConfig("frame rate must be positive".into()));
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameDesc {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub color_space: ColorSpace,
    pub memory: MemoryDomain,
    byte_len: usize,
}

impl FrameDesc {
    pub fn host_nv12(width: u32, height: u32, color_space: ColorSpace) -> Result<Self> {
        if width == 0 || height == 0 || width % 2 != 0 || height % 2 != 0 {
            return Err(Error::UnsupportedFrame(format!(
                "NV12 requires non-zero even dimensions, got {width}x{height}"
            )));
        }
        let byte_len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(3))
            .map(|bytes| bytes / 2)
            .ok_or_else(|| Error::UnsupportedFrame("NV12 frame size overflows usize".into()))?;
        Ok(Self {
            width,
            height,
            format: PixelFormat::Nv12,
            color_space,
            memory: MemoryDomain::Host,
            byte_len,
        })
    }

    pub fn byte_len(&self) -> usize {
        self.byte_len
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostFrame {
    data: Vec<u8>,
}

impl HostFrame {
    pub fn new_zeroed(desc: &FrameDesc) -> Self {
        Self {
            data: vec![0; desc.byte_len()],
        }
    }
    pub fn from_nv12(desc: &FrameDesc, data: Vec<u8>) -> Result<Self> {
        if data.len() != desc.byte_len() {
            return Err(Error::UnsupportedFrame(format!(
                "NV12 buffer is {} bytes; expected {}",
                data.len(),
                desc.byte_len()
            )));
        }
        Ok(Self { data })
    }
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }
    pub fn planes(&self, desc: &FrameDesc) -> (&[u8], &[u8]) {
        self.data
            .split_at(desc.width as usize * desc.height as usize)
    }
    pub fn planes_mut(&mut self, desc: &FrameDesc) -> (&mut [u8], &mut [u8]) {
        self.data
            .split_at_mut(desc.width as usize * desc.height as usize)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoFrame {
    desc: FrameDesc,
    pts: Option<i64>,
    storage: HostFrame,
}

impl VideoFrame {
    pub fn new_host(desc: FrameDesc, pts: Option<i64>, storage: HostFrame) -> Result<Self> {
        if storage.as_slice().len() != desc.byte_len() {
            return Err(Error::UnsupportedFrame(
                "storage does not match descriptor".into(),
            ));
        }
        Ok(Self { desc, pts, storage })
    }
    pub fn desc(&self) -> &FrameDesc {
        &self.desc
    }
    pub fn pts(&self) -> Option<i64> {
        self.pts
    }
    pub fn host(&self) -> &HostFrame {
        &self.storage
    }
    pub fn host_mut(&mut self) -> &mut HostFrame {
        &mut self.storage
    }
    pub fn into_host(self) -> HostFrame {
        self.storage
    }
}
