use asciiflow_core::{Error, PixelFormat, Result, VideoFrame, glyph_lookup_table};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AsciiCell {
    pub glyph: u16,
    pub y: u16,
    pub u: u16,
    pub v: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CellGrid {
    pub width: u32,
    pub height: u32,
    pub cells: Vec<AsciiCell>,
}

/// Retains its original API name; maps both NV12 and P010LE host frames.
pub struct Nv12Mapper {
    glyph_lut: [u32; 256],
}

impl Nv12Mapper {
    pub fn new(charset: &str) -> Result<Self> {
        let glyph_count = charset.chars().count();
        if glyph_count == 0 || glyph_count > u16::MAX as usize {
            return Err(Error::Cpu("invalid charset length".into()));
        }
        Ok(Self {
            glyph_lut: glyph_lookup_table(glyph_count),
        })
    }

    pub fn map(&self, frame: &VideoFrame, grid_width: u32, grid_height: u32) -> Result<CellGrid> {
        let desc = frame.desc();
        if grid_width == 0
            || grid_height == 0
            || grid_width > desc.width
            || grid_height > desc.height
        {
            return Err(Error::Cpu(
                "cell grid must fit inside the source frame".into(),
            ));
        }
        match desc.format {
            PixelFormat::Nv12 => self.map_impl::<false>(frame, grid_width, grid_height),
            PixelFormat::P010Le => self.map_impl::<true>(frame, grid_width, grid_height),
        }
    }

    fn map_impl<const P010: bool>(
        &self,
        frame: &VideoFrame,
        grid_width: u32,
        grid_height: u32,
    ) -> Result<CellGrid> {
        let desc = frame.desc();
        let (y_plane, uv_plane) = frame.host().planes(desc);
        let stride = desc.width as usize;
        let cell_count = (grid_width as usize)
            .checked_mul(grid_height as usize)
            .ok_or_else(|| Error::Cpu("cell count overflows usize".into()))?;
        let mut cells = Vec::with_capacity(cell_count);
        for cell_y in 0..grid_height {
            let y0 = cell_y as usize * desc.height as usize / grid_height as usize;
            let y1 =
                ((cell_y + 1) as usize * desc.height as usize / grid_height as usize).max(y0 + 1);
            for cell_x in 0..grid_width {
                let x0 = cell_x as usize * desc.width as usize / grid_width as usize;
                let x1 =
                    ((cell_x + 1) as usize * desc.width as usize / grid_width as usize).max(x0 + 1);
                let cell_pixels = (x1 - x0) as u64 * (y1 - y0) as u64;
                if cell_pixels > u64::MAX / 1023 {
                    return Err(Error::Cpu("cell luma sum exceeds u64".into()));
                }
                let mut y_sum = 0u64;
                for row in y0..y1 {
                    if P010 {
                        for x in x0..x1 {
                            y_sum += read_sample::<true>(y_plane, row * stride + x) as u64;
                        }
                    } else {
                        for &sample in &y_plane[row * stride + x0..row * stride + x1] {
                            y_sum += sample as u64;
                        }
                    }
                }
                let average_y = (y_sum / cell_pixels) as u16;
                let mut u_sum = 0u64;
                let mut v_sum = 0u64;
                let uv_y0 = y0 / 2;
                let uv_y1 = y1.div_ceil(2);
                let uv_x0 = x0 & !1;
                let uv_x1 = (x1 + 1).min(stride) & !1;
                let uv_count = ((uv_x1 - uv_x0) / 2) as u64 * (uv_y1 - uv_y0) as u64;
                for row in uv_y0..uv_y1 {
                    let base = row * stride;
                    for x in (uv_x0..uv_x1).step_by(2) {
                        u_sum += read_sample::<P010>(uv_plane, base + x) as u64;
                        v_sum += read_sample::<P010>(uv_plane, base + x + 1) as u64;
                    }
                }
                let glyph_luma = if P010 { average_y >> 2 } else { average_y };
                let glyph = self.glyph_lut[glyph_luma as usize] as u16;
                cells.push(AsciiCell {
                    glyph,
                    y: average_y,
                    u: u_sum
                        .checked_div(uv_count)
                        .map_or(desc.format.neutral_chroma(), |value| value as u16),
                    v: v_sum
                        .checked_div(uv_count)
                        .map_or(desc.format.neutral_chroma(), |value| value as u16),
                });
            }
        }
        Ok(CellGrid {
            width: grid_width,
            height: grid_height,
            cells,
        })
    }
}

fn read_sample<const P010: bool>(plane: &[u8], index: usize) -> u16 {
    if P010 {
        let offset = index * 2;
        u16::from_le_bytes([plane[offset], plane[offset + 1]]) >> 6
    } else {
        plane[index] as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asciiflow_core::{ColorSpace, FrameDesc, HostFrame};
    #[test]
    fn fixed_nv12_maps_to_expected_glyphs() {
        let desc = FrameDesc::host_nv12(4, 2, ColorSpace::default()).unwrap();
        let data = vec![16, 16, 235, 235, 16, 16, 235, 235, 128, 128, 128, 128];
        let frame = VideoFrame::new_host(
            desc.clone(),
            Some(7),
            HostFrame::from_nv12(&desc, data).unwrap(),
        )
        .unwrap();
        let grid = Nv12Mapper::new("@ ").unwrap().map(&frame, 2, 1).unwrap();
        assert_eq!(
            grid.cells.iter().map(|c| c.glyph).collect::<Vec<_>>(),
            vec![0, 1]
        );
    }

    #[test]
    fn exact_nv12_expansion_preserves_glyphs_and_ten_bit_codes() {
        let color_space = ColorSpace::default();
        let nv12_desc = FrameDesc::host_nv12(16, 16, color_space).unwrap();
        let nv12_data: Vec<u8> = (0..nv12_desc.byte_len())
            .map(|index| (index.wrapping_mul(37) % 256) as u8)
            .collect();
        let nv12 = VideoFrame::new_host(
            nv12_desc.clone(),
            Some(9),
            HostFrame::from_nv12(&nv12_desc, nv12_data.clone()).unwrap(),
        )
        .unwrap();
        let p010_desc = FrameDesc::host_p010_le(16, 16, color_space).unwrap();
        let p010_data: Vec<u8> = nv12_data
            .iter()
            .flat_map(|&sample| ((sample as u16) << 8).to_le_bytes())
            .collect();
        let p010 = VideoFrame::new_host(
            p010_desc.clone(),
            Some(9),
            HostFrame::from_p010_le(&p010_desc, p010_data).unwrap(),
        )
        .unwrap();
        let mapper = Nv12Mapper::new("@%#*+=-:. ").unwrap();
        let mapped_8 = mapper.map(&nv12, 5, 3).unwrap();
        let mapped_10 = mapper.map(&p010, 5, 3).unwrap();
        for (cell8, cell10) in mapped_8.cells.iter().zip(&mapped_10.cells) {
            assert_eq!(cell8.glyph, cell10.glyph);
            assert_eq!(cell10.y >> 2, cell8.y);
            assert_eq!(cell10.u >> 2, cell8.u);
            assert_eq!(cell10.v >> 2, cell8.v);
        }
    }

    #[test]
    fn p010_mapping_retains_low_active_luma_and_chroma_bits() {
        let desc = FrameDesc::host_p010_le(2, 2, ColorSpace::default()).unwrap();
        let samples = [512u16, 513, 514, 515, 513, 514];
        let bytes = samples
            .into_iter()
            .flat_map(|sample| (sample << 6).to_le_bytes())
            .collect();
        let frame = VideoFrame::new_host(
            desc.clone(),
            None,
            HostFrame::from_p010_le(&desc, bytes).unwrap(),
        )
        .unwrap();
        let cell = Nv12Mapper::new("@ ")
            .unwrap()
            .map(&frame, 1, 1)
            .unwrap()
            .cells[0];
        assert_eq!((cell.y, cell.u, cell.v), (513, 513, 514));
    }
}
