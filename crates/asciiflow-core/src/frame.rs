use crate::{Error, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelFormat {
    Nv12,
    P010Le,
}

impl PixelFormat {
    pub const fn bytes_per_sample(self) -> usize {
        match self {
            Self::Nv12 => 1,
            Self::P010Le => 2,
        }
    }

    pub const fn neutral_chroma(self) -> u16 {
        match self {
            Self::Nv12 => 128,
            Self::P010Le => 512,
        }
    }

    pub fn y_plane_len(self, width: u32, height: u32) -> Result<usize> {
        (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(self.bytes_per_sample()))
            .ok_or_else(|| Error::UnsupportedFrame("luma plane size overflows usize".into()))
    }

    pub fn uv_plane_len(self, width: u32, height: u32) -> Result<usize> {
        Ok(self.y_plane_len(width, height)? / 2)
    }

    pub fn frame_byte_len(self, width: u32, height: u32) -> Result<usize> {
        self.y_plane_len(width, height)?
            .checked_add(self.uv_plane_len(width, height)?)
            .ok_or_else(|| Error::UnsupportedFrame("frame size overflows usize".into()))
    }
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
        Self::host(width, height, PixelFormat::Nv12, color_space)
    }

    pub fn host_p010_le(width: u32, height: u32, color_space: ColorSpace) -> Result<Self> {
        Self::host(width, height, PixelFormat::P010Le, color_space)
    }

    fn host(width: u32, height: u32, format: PixelFormat, color_space: ColorSpace) -> Result<Self> {
        if width == 0 || height == 0 || width % 2 != 0 || height % 2 != 0 {
            return Err(Error::UnsupportedFrame(format!(
                "{format:?} requires non-zero even dimensions, got {width}x{height}"
            )));
        }
        let byte_len = format.frame_byte_len(width, height)?;
        Ok(Self {
            width,
            height,
            format,
            color_space,
            memory: MemoryDomain::Host,
            byte_len,
        })
    }

    pub fn byte_len(&self) -> usize {
        self.byte_len
    }

    pub fn y_plane_len(&self) -> usize {
        // Both formats have a 2:1 luma/chroma byte ratio.
        self.byte_len - self.byte_len / 3
    }

    pub fn uv_plane_len(&self) -> usize {
        self.byte_len - self.y_plane_len()
    }

    pub fn y_stride(&self) -> usize {
        self.width as usize * self.format.bytes_per_sample()
    }

    pub fn uv_stride(&self) -> usize {
        self.y_stride()
    }

    pub fn validate_layout(&self) -> Result<()> {
        let expected = Self::host(self.width, self.height, self.format, self.color_space)?;
        if self.memory != MemoryDomain::Host || self.byte_len != expected.byte_len {
            return Err(Error::UnsupportedFrame(
                "frame descriptor layout is inconsistent".into(),
            ));
        }
        Ok(())
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
    pub fn try_new_zeroed(desc: &FrameDesc) -> Result<Self> {
        desc.validate_layout()?;
        let mut data = Vec::new();
        data.try_reserve_exact(desc.byte_len())
            .map_err(|error| Error::UnsupportedFrame(format!("cannot allocate frame: {error}")))?;
        data.resize(desc.byte_len(), 0);
        Ok(Self { data })
    }
    pub fn from_bytes(desc: &FrameDesc, data: Vec<u8>) -> Result<Self> {
        desc.validate_layout()?;
        if data.len() != desc.byte_len() {
            return Err(Error::UnsupportedFrame(format!(
                "{:?} buffer is {} bytes; expected {}",
                desc.format,
                data.len(),
                desc.byte_len()
            )));
        }
        Ok(Self { data })
    }
    pub fn from_nv12(desc: &FrameDesc, data: Vec<u8>) -> Result<Self> {
        if desc.format != PixelFormat::Nv12 {
            return Err(Error::UnsupportedFrame("expected NV12 descriptor".into()));
        }
        Self::from_bytes(desc, data)
    }
    pub fn from_p010_le(desc: &FrameDesc, data: Vec<u8>) -> Result<Self> {
        if desc.format != PixelFormat::P010Le {
            return Err(Error::UnsupportedFrame("expected P010LE descriptor".into()));
        }
        Self::from_bytes(desc, data)
    }
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }
    pub fn planes(&self, desc: &FrameDesc) -> (&[u8], &[u8]) {
        self.data.split_at(desc.y_plane_len())
    }
    pub fn planes_mut(&mut self, desc: &FrameDesc) -> (&mut [u8], &mut [u8]) {
        self.data.split_at_mut(desc.y_plane_len())
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
        desc.validate_layout()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p010_le_layout_and_sample_packing() {
        for (width, height) in [(2, 2), (4, 4), (16, 16), (1920, 1080)] {
            let desc = FrameDesc::host_p010_le(width, height, ColorSpace::default()).unwrap();
            let pixels = width as usize * height as usize;
            assert_eq!(desc.y_stride(), width as usize * 2);
            assert_eq!(desc.uv_stride(), width as usize * 2);
            assert_eq!(desc.y_plane_len(), pixels * 2);
            assert_eq!(desc.uv_plane_len(), pixels);
            assert_eq!(desc.byte_len(), pixels * 3);
            let storage = HostFrame::new_zeroed(&desc);
            let (y, uv) = storage.planes(&desc);
            assert_eq!(y.len(), desc.y_plane_len());
            assert_eq!(uv.len(), desc.uv_plane_len());
        }
        for code in [0u16, 1, 2, 3, 4, 511, 512, 1020, 1023] {
            let bytes = (code << 6).to_le_bytes();
            let stored = u16::from_le_bytes(bytes);
            assert_eq!(stored >> 6, code);
            assert_eq!(stored & 0x003f, 0);
        }
    }

    #[test]
    fn p010_le_rejects_invalid_geometry_size_and_mismatched_format() {
        for (width, height) in [(0, 2), (2, 0), (1, 2), (2, 3)] {
            assert!(FrameDesc::host_p010_le(width, height, ColorSpace::default()).is_err());
        }
        assert!(
            FrameDesc::host_p010_le(u32::MAX - 1, u32::MAX - 1, ColorSpace::default()).is_err()
        );
        let desc = FrameDesc::host_p010_le(2, 2, ColorSpace::default()).unwrap();
        assert!(HostFrame::from_p010_le(&desc, vec![0; desc.byte_len() - 1]).is_err());
        assert!(HostFrame::from_nv12(&desc, vec![0; desc.byte_len()]).is_err());
        let mut altered = desc.clone();
        altered.format = PixelFormat::Nv12;
        assert!(HostFrame::from_bytes(&altered, vec![0; desc.byte_len()]).is_err());
    }
}
