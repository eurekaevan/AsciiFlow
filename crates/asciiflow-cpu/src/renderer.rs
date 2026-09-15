use crate::CellGrid;
use asciiflow_core::{Error, FrameDesc, HostFrame, Result, VideoFrame};
use asciiflow_font::GlyphAtlas;

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
        if grid.cells.len() != grid.width as usize * grid.height as usize {
            return Err(Error::Cpu("cell grid storage is incomplete".into()));
        }
        let mut storage = HostFrame::new_zeroed(&desc);
        let (y_plane, uv_plane) = storage.planes_mut(&desc);
        y_plane.fill(16);
        uv_plane.fill(128);
        let width = desc.width as usize;
        let height = desc.height as usize;
        for py in 0..height {
            let cy = py * grid.height as usize / height;
            let local_y = (py * grid.height as usize * self.atlas.height() as usize / height)
                % self.atlas.height() as usize;
            for px in 0..width {
                let cx = px * grid.width as usize / width;
                let cell = grid.cells[cy * grid.width as usize + cx];
                let local_x = (px * grid.width as usize * self.atlas.width() as usize / width)
                    % self.atlas.width() as usize;
                let alpha = self.atlas.glyph(cell.glyph as usize)
                    [local_y * self.atlas.width() as usize + local_x]
                    as u16;
                let foreground = if color { cell.y } else { 235 };
                y_plane[py * width + px] = blend(16, foreground, alpha);
            }
        }
        for py in (0..height).step_by(2) {
            for px in (0..width).step_by(2) {
                let cy = py * grid.height as usize / height;
                let cx = px * grid.width as usize / width;
                let cell = grid.cells[cy * grid.width as usize + cx];
                if color {
                    let mut alpha = 0u32;
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let sample_y =
                                ((py + dy) * grid.height as usize * self.atlas.height() as usize
                                    / height)
                                    % self.atlas.height() as usize;
                            let sample_x =
                                ((px + dx) * grid.width as usize * self.atlas.width() as usize
                                    / width)
                                    % self.atlas.width() as usize;
                            alpha += self.atlas.glyph(cell.glyph as usize)
                                [sample_y * self.atlas.width() as usize + sample_x]
                                as u32;
                        }
                    }
                    let offset = (py / 2) * width + px;
                    let average = (alpha / 4) as u16;
                    uv_plane[offset] = blend(128, cell.u, average);
                    uv_plane[offset + 1] = blend(128, cell.v, average);
                }
            }
        }
        VideoFrame::new_host(desc, pts, storage)
    }
}

fn blend(background: u8, foreground: u8, alpha: u16) -> u8 {
    ((background as u16 * (255 - alpha) + foreground as u16 * alpha + 127) / 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AsciiCell;
    use asciiflow_core::ColorSpace;
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
}
