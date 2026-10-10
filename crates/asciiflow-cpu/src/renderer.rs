use crate::CellGrid;
use asciiflow_core::{Error, FrameDesc, HostFrame, PixelFormat, Result, VideoFrame};
use asciiflow_font::GlyphAtlas;

/// Retains its original API name; renders into the descriptor's host format.
pub struct Nv12Renderer<'a> {
    atlas: &'a GlyphAtlas,
}
impl<'a> Nv12Renderer<'a> {
    pub fn new(atlas: &'a GlyphAtlas) -> Self {
        Self { atlas }
    }

    pub fn render(
        &self,
        grid: &CellGrid,
        desc: FrameDesc,
        pts: Option<i64>,
        color: bool,
    ) -> Result<VideoFrame> {
        desc.validate_layout()?;
        if grid.width == 0
            || grid.height == 0
            || grid.width > desc.width
            || grid.height > desc.height
        {
            return Err(Error::Cpu(
                "cell grid must fit inside the source frame".into(),
            ));
        }
        if grid.cells.len() != grid.width as usize * grid.height as usize {
            return Err(Error::Cpu("cell grid storage is incomplete".into()));
        }
        if grid
            .cells
            .iter()
            .any(|cell| cell.glyph as usize >= self.atlas.glyph_count())
        {
            return Err(Error::Cpu("cell glyph index exceeds atlas".into()));
        }
        let max_code = match desc.format {
            PixelFormat::Nv12 => 255,
            PixelFormat::P010Le => 1023,
        };
        if grid
            .cells
            .iter()
            .any(|cell| cell.y > max_code || cell.u > max_code || cell.v > max_code)
        {
            return Err(Error::Cpu("cell color exceeds pixel format range".into()));
        }
        match desc.format {
            PixelFormat::Nv12 => self.render_impl::<false>(grid, desc, pts, color),
            PixelFormat::P010Le => self.render_impl::<true>(grid, desc, pts, color),
        }
    }

    fn render_impl<const P010: bool>(
        &self,
        grid: &CellGrid,
        desc: FrameDesc,
        pts: Option<i64>,
        color: bool,
    ) -> Result<VideoFrame> {
        let (background_y, neutral_uv, monochrome_y) = if P010 {
            (64u32, 512u32, 940u32)
        } else {
            (16u32, 128u32, 235u32)
        };
        let mut storage = if P010 {
            HostFrame::try_new_zeroed(&desc)?
        } else {
            HostFrame::new_zeroed(&desc)
        };
        let (y_plane, uv_plane) = storage.planes_mut(&desc);
        fill_samples::<P010>(y_plane, background_y);
        fill_samples::<P010>(uv_plane, neutral_uv);
        let width = desc.width as usize;
        let height = desc.height as usize;
        for py in 0..height {
            let [cy, local_y] = render_coordinate(
                py,
                height,
                grid.height as usize,
                self.atlas.height() as usize,
            );
            for px in 0..width {
                let [cx, local_x] =
                    render_coordinate(px, width, grid.width as usize, self.atlas.width() as usize);
                let cell = grid.cells[cy * grid.width as usize + cx];
                let alpha = self.atlas.glyph(cell.glyph as usize)
                    [local_y * self.atlas.width() as usize + local_x]
                    as u32;
                let foreground = if color { cell.y as u32 } else { monochrome_y };
                write_sample::<P010>(
                    y_plane,
                    py * width + px,
                    blend(background_y, foreground, alpha),
                );
            }
        }
        for py in (0..height).step_by(2) {
            for px in (0..width).step_by(2) {
                let [cy, _] = render_coordinate(
                    py,
                    height,
                    grid.height as usize,
                    self.atlas.height() as usize,
                );
                let [cx, _] =
                    render_coordinate(px, width, grid.width as usize, self.atlas.width() as usize);
                let cell = grid.cells[cy * grid.width as usize + cx];
                if color {
                    let mut alpha = 0u32;
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let [_, sample_y] = render_coordinate(
                                py + dy,
                                height,
                                grid.height as usize,
                                self.atlas.height() as usize,
                            );
                            let [_, sample_x] = render_coordinate(
                                px + dx,
                                width,
                                grid.width as usize,
                                self.atlas.width() as usize,
                            );
                            alpha += self.atlas.glyph(cell.glyph as usize)
                                [sample_y * self.atlas.width() as usize + sample_x]
                                as u32;
                        }
                    }
                    let offset = (py / 2) * width + px;
                    let average = alpha / 4;
                    write_sample::<P010>(
                        uv_plane,
                        offset,
                        blend(neutral_uv, cell.u as u32, average),
                    );
                    write_sample::<P010>(
                        uv_plane,
                        offset + 1,
                        blend(neutral_uv, cell.v as u32, average),
                    );
                }
            }
        }
        VideoFrame::new_host(desc, pts, storage)
    }
}

// Inverse of the mapper's floor boundaries, with glyph coordinates local to
// that same cell. Use u64 products for the public u32 frame/atlas dimensions.
fn render_coordinate(pixel: usize, extent: usize, count: usize, atlas: usize) -> [usize; 2] {
    let (pixel, extent, count, atlas) = (pixel as u64, extent as u64, count as u64, atlas as u64);
    let cell = ((pixel + 1) * count - 1) / extent;
    let start = cell * extent / count;
    let end = (cell + 1) * extent / count;
    [
        cell as usize,
        ((pixel - start) * atlas / (end - start)) as usize,
    ]
}

fn blend(background: u32, foreground: u32, alpha: u32) -> u32 {
    (background * (255 - alpha) + foreground * alpha + 127) / 255
}

fn write_sample<const P010: bool>(plane: &mut [u8], index: usize, code: u32) {
    if P010 {
        let offset = index * 2;
        plane[offset..offset + 2].copy_from_slice(&((code as u16) << 6).to_le_bytes());
    } else {
        plane[index] = code as u8;
    }
}

fn fill_samples<const P010: bool>(plane: &mut [u8], code: u32) {
    if P010 {
        let word = ((code as u16) << 6).to_le_bytes();
        for sample in plane.as_chunks_mut::<2>().0 {
            sample.copy_from_slice(&word);
        }
    } else {
        plane.fill(code as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AsciiCell;
    use asciiflow_core::ColorSpace;

    #[test]
    fn zero_or_oversized_grids_are_errors_not_panics() {
        let atlas = GlyphAtlas::builtin("builtin-8x8", " ").unwrap();
        for (width, height) in [(0, 1), (1, 0), (0, 0), (3, 1), (1, 3), (u32::MAX, u32::MAX)] {
            let grid = CellGrid {
                width,
                height,
                cells: Vec::new(),
            };
            let desc = FrameDesc::host_nv12(2, 2, ColorSpace::default()).unwrap();
            assert!(matches!(
                Nv12Renderer::new(&atlas).render(&grid, desc, None, false),
                Err(Error::Cpu(_))
            ));
        }
    }

    #[test]
    fn uneven_grid_render_uses_the_same_pixels_as_mapping() {
        let input_y = [16u16, 16, 16, 100, 100, 100, 200, 200, 200, 200];
        let atlas = GlyphAtlas::from_r8(1, 1, 1, vec![255]).unwrap();
        for format in [PixelFormat::Nv12, PixelFormat::P010Le] {
            let desc = match format {
                PixelFormat::Nv12 => FrameDesc::host_nv12(10, 2, ColorSpace::default()),
                PixelFormat::P010Le => FrameDesc::host_p010_le(10, 2, ColorSpace::default()),
            }
            .unwrap();
            let mut codes = input_y.repeat(2);
            codes.extend([128; 10]);
            let storage = match format {
                PixelFormat::Nv12 => {
                    HostFrame::from_nv12(&desc, codes.iter().map(|&x| x as u8).collect())
                }
                PixelFormat::P010Le => HostFrame::from_p010_le(
                    &desc,
                    codes.iter().flat_map(|&x| (x << 8).to_le_bytes()).collect(),
                ),
            }
            .unwrap();
            let input = VideoFrame::new_host(desc.clone(), Some(7), storage).unwrap();
            let grid = crate::Nv12Mapper::new("@")
                .unwrap()
                .map(&input, 3, 1)
                .unwrap();
            let output = Nv12Renderer::new(&atlas)
                .render(&grid, desc, Some(7), true)
                .unwrap();
            assert_eq!(output, input, "{format:?}");
        }
    }

    #[test]
    fn uneven_cells_use_local_glyph_coordinates_on_both_axes() {
        for (width, height, gw, gh, aw, ah) in [(10, 2, 3, 1, 3, 1), (2, 10, 1, 3, 1, 3)] {
            let atlas = GlyphAtlas::from_r8(aw, ah, 1, vec![0, 128, 255]).unwrap();
            let grid = CellGrid {
                width: gw,
                height: gh,
                cells: vec![
                    AsciiCell {
                        glyph: 0,
                        y: 235,
                        u: 128,
                        v: 128
                    };
                    3
                ],
            };
            let desc = FrameDesc::host_nv12(width, height, ColorSpace::default()).unwrap();
            let output = Nv12Renderer::new(&atlas)
                .render(&grid, desc.clone(), None, false)
                .unwrap();
            let y = output.host().planes(&desc).0;
            let actual: Vec<_> = (0..10)
                .map(|i| y[if width == 10 { i } else { i * 2 }])
                .collect();
            assert_eq!(actual, [16, 126, 235, 16, 126, 235, 16, 16, 126, 235]);
        }
    }

    #[test]
    fn out_of_range_glyph_is_an_error_not_a_panic() {
        let atlas = GlyphAtlas::builtin("builtin-8x8", " ").unwrap();
        let grid = CellGrid {
            width: 1,
            height: 1,
            cells: vec![AsciiCell {
                glyph: 1,
                y: 100,
                u: 128,
                v: 128,
            }],
        };
        assert!(
            Nv12Renderer::new(&atlas)
                .render(
                    &grid,
                    FrameDesc::host_nv12(8, 8, ColorSpace::default()).unwrap(),
                    None,
                    true
                )
                .is_err()
        );
    }
    #[test]
    fn rendering_is_deterministic_nv12() {
        let atlas = GlyphAtlas::builtin("builtin-8x8", "@ ").unwrap();
        let renderer = Nv12Renderer::new(&atlas);
        let grid = CellGrid {
            width: 1,
            height: 1,
            cells: vec![AsciiCell {
                glyph: 0,
                y: 180,
                u: 90,
                v: 170,
            }],
        };
        let desc = FrameDesc::host_nv12(8, 8, ColorSpace::default()).unwrap();
        let a = renderer.render(&grid, desc.clone(), Some(1), true).unwrap();
        let b = renderer.render(&grid, desc, Some(1), true).unwrap();
        assert_eq!(a, b);
        assert!(a.host().as_slice().iter().any(|&v| v != 16 && v != 128));
    }

    #[test]
    fn p010_render_retains_luma_and_chroma_precision_with_zero_padding() {
        let atlas = GlyphAtlas::from_r8(2, 2, 1, vec![255; 4]).unwrap();
        let renderer = Nv12Renderer::new(&atlas);
        let desc = FrameDesc::host_p010_le(2, 2, ColorSpace::default()).unwrap();
        for (y, u, v) in [(512, 512, 512), (513, 513, 514), (514, 514, 515)] {
            let grid = CellGrid {
                width: 1,
                height: 1,
                cells: vec![AsciiCell { glyph: 0, y, u, v }],
            };
            let frame = renderer.render(&grid, desc.clone(), Some(7), true).unwrap();
            assert_eq!(frame.desc().color_space, ColorSpace::default());
            assert_eq!(frame.pts(), Some(7));
            let (y_plane, uv_plane) = frame.host().planes(&desc);
            for bytes in y_plane.as_chunks::<2>().0 {
                let word = u16::from_le_bytes([bytes[0], bytes[1]]);
                assert_eq!(word >> 6, y);
                assert_eq!(word & 0x3f, 0);
            }
            assert_eq!(u16::from_le_bytes([uv_plane[0], uv_plane[1]]) >> 6, u);
            assert_eq!(u16::from_le_bytes([uv_plane[2], uv_plane[3]]) >> 6, v);
            assert!(
                uv_plane
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .all(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]) & 0x3f == 0)
            );
        }
    }

    #[test]
    fn p010_blend_operates_on_ten_bit_codes() {
        let atlas = GlyphAtlas::from_r8(2, 2, 1, vec![128; 4]).unwrap();
        let renderer = Nv12Renderer::new(&atlas);
        let desc = FrameDesc::host_p010_le(2, 2, ColorSpace::default()).unwrap();
        let render = |code| {
            renderer
                .render(
                    &CellGrid {
                        width: 1,
                        height: 1,
                        cells: vec![AsciiCell {
                            glyph: 0,
                            y: code,
                            u: code,
                            v: code,
                        }],
                    },
                    desc.clone(),
                    None,
                    true,
                )
                .unwrap()
        };
        let a = render(512);
        let b = render(515);
        assert_ne!(a.host().as_slice(), b.host().as_slice());
        assert!(
            b.host()
                .as_slice()
                .as_chunks::<2>()
                .0
                .iter()
                .all(|bytes| { u16::from_le_bytes([bytes[0], bytes[1]]) & 0x3f == 0 })
        );
    }
}
