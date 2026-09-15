use asciiflow_core::{Error, Result, VideoFrame, glyph_lookup_table};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AsciiCell {
    pub glyph: u16,
    pub y: u8,
    pub u: u8,
    pub v: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CellGrid {
    pub width: u32,
    pub height: u32,
    pub cells: Vec<AsciiCell>,
}

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
        let (y_plane, uv_plane) = frame.host().planes(desc);
        let stride = desc.width as usize;
        let mut cells = Vec::with_capacity(grid_width as usize * grid_height as usize);
        for cell_y in 0..grid_height {
            let y0 = cell_y as usize * desc.height as usize / grid_height as usize;
            let y1 =
                ((cell_y + 1) as usize * desc.height as usize / grid_height as usize).max(y0 + 1);
            for cell_x in 0..grid_width {
                let x0 = cell_x as usize * desc.width as usize / grid_width as usize;
                let x1 =
                    ((cell_x + 1) as usize * desc.width as usize / grid_width as usize).max(x0 + 1);
                let mut y_sum = 0u64;
                let mut y_count = 0u64;
                for row in y0..y1 {
                    for &value in &y_plane[row * stride + x0..row * stride + x1] {
                        y_sum += value as u64;
                        y_count += 1;
                    }
                }
                let average_y = (y_sum / y_count) as u8;
                let mut u_sum = 0u64;
                let mut v_sum = 0u64;
                let mut uv_count = 0u64;
                let uv_y0 = y0 / 2;
                let uv_y1 = y1.div_ceil(2);
                let uv_x0 = x0 & !1;
                let uv_x1 = (x1 + 1).min(stride) & !1;
                for row in uv_y0..uv_y1 {
                    let base = row * stride;
                    for x in (uv_x0..uv_x1).step_by(2) {
                        u_sum += uv_plane[base + x] as u64;
                        v_sum += uv_plane[base + x + 1] as u64;
                        uv_count += 1;
                    }
                }
                let glyph = self.glyph_lut[average_y as usize] as u16;
                cells.push(AsciiCell {
                    glyph,
                    y: average_y,
                    u: u_sum.checked_div(uv_count).unwrap_or(128) as u8,
                    v: v_sum.checked_div(uv_count).unwrap_or(128) as u8,
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
}
