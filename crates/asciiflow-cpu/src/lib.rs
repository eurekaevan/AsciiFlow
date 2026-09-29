pub mod hdr;
mod mapper;
mod renderer;
pub mod tone_map;

use asciiflow_core::{
    AsciiBackend, AsciiConfig, BackendOutput, BackendTimings, Error, Result, VideoFrame,
};
use asciiflow_font::GlyphAtlas;
use std::time::Instant;

pub use mapper::{AsciiCell, CellGrid, Nv12Mapper};
pub use renderer::Nv12Renderer;

pub struct CpuAsciiBackend {
    supplied: bool,
    atlas_key: Option<(String, String)>,
    atlas: Option<GlyphAtlas>,
}

impl CpuAsciiBackend {
    pub fn with_atlas(atlas: GlyphAtlas, config: &AsciiConfig) -> Self {
        Self {
            supplied: true,
            atlas_key: Some((config.font.clone(), config.charset.clone())),
            atlas: Some(atlas),
        }
    }
    pub fn new() -> Self {
        Self {
            supplied: false,
            atlas_key: None,
            atlas: None,
        }
    }
}
impl Default for CpuAsciiBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AsciiBackend for CpuAsciiBackend {
    fn process(&mut self, input: VideoFrame, config: &AsciiConfig) -> Result<BackendOutput> {
        let total_started = Instant::now();
        let (grid_width, grid_height) =
            config.resolved_grid(input.desc().width, input.desc().height)?;
        let key = (config.font.clone(), config.charset.clone());
        if self.supplied && self.atlas_key.as_ref() != Some(&key) {
            return Err(Error::Cpu(
                "supplied atlas font/ramp identity changed".into(),
            ));
        }
        if self.atlas_key.as_ref() != Some(&key) {
            self.atlas = Some(
                GlyphAtlas::builtin(&config.font, &config.charset)
                    .map_err(|e| Error::Cpu(e.to_string()))?,
            );
            self.atlas_key = Some(key);
        }
        let mapping_started = Instant::now();
        if self
            .atlas
            .as_ref()
            .is_none_or(|atlas| atlas.glyph_count() != config.charset.chars().count())
        {
            return Err(Error::Cpu(
                "atlas glyph count does not match the ramp".into(),
            ));
        }
        let grid = Nv12Mapper::new(&config.charset)?.map(&input, grid_width, grid_height)?;
        let mapping_time = mapping_started.elapsed();
        let render_started = Instant::now();
        let frame = Nv12Renderer::new(self.atlas.as_ref().expect("atlas initialized")).render(
            &grid,
            input.desc().clone(),
            input.pts(),
            config.color,
        )?;
        Ok(BackendOutput {
            frame,
            timings: BackendTimings {
                mapping: mapping_time,
                render: render_started.elapsed(),
                backend_wall: total_started.elapsed(),
                ..BackendTimings::default()
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asciiflow_core::{ColorSpace, FrameDesc, HostFrame, PixelFormat};

    #[test]
    fn default_ramp_keeps_black_black_and_renders_white_brighter() {
        for format in [PixelFormat::Nv12, PixelFormat::P010Le] {
            let desc = match format {
                PixelFormat::Nv12 => FrameDesc::host_nv12(16, 8, ColorSpace::default()),
                PixelFormat::P010Le => FrameDesc::host_p010_le(16, 8, ColorSpace::default()),
            }
            .unwrap();
            let (black, white, neutral) = match format {
                PixelFormat::Nv12 => (16u16, 235u16, 128u16),
                PixelFormat::P010Le => (64u16, 940u16, 512u16),
            };
            let mut host = HostFrame::new_zeroed(&desc);
            let (y_plane, uv_plane) = host.planes_mut(&desc);
            for row in y_plane.chunks_exact_mut(16 * format.bytes_per_sample()) {
                for x in 0..16 {
                    let value = if x < 8 { black } else { white };
                    if format == PixelFormat::Nv12 {
                        row[x] = value as u8;
                    } else {
                        row[2 * x..2 * x + 2].copy_from_slice(&(value << 6).to_le_bytes());
                    }
                }
            }
            if format == PixelFormat::Nv12 {
                uv_plane.fill(neutral as u8);
            } else {
                for sample in uv_plane.chunks_exact_mut(2) {
                    sample.copy_from_slice(&(neutral << 6).to_le_bytes());
                }
            }
            let input = VideoFrame::new_host(desc.clone(), None, host).unwrap();
            for color in [false, true] {
                let config = AsciiConfig {
                    grid_width: 2,
                    grid_height: Some(1),
                    color,
                    ..AsciiConfig::default()
                };
                let grid = Nv12Mapper::new(&config.charset)
                    .unwrap()
                    .map(&input, 2, 1)
                    .unwrap();
                assert_eq!(grid.cells[0].glyph, 0);
                assert_eq!(
                    grid.cells[1].glyph as usize,
                    config.charset.chars().count() - 1
                );

                let rendered = CpuAsciiBackend::new()
                    .process(input.clone(), &config)
                    .unwrap()
                    .frame;
                let (y_plane, _) = rendered.host().planes(&desc);
                let samples: Vec<u16> = if format == PixelFormat::Nv12 {
                    y_plane.iter().map(|&y| y as u16).collect()
                } else {
                    y_plane
                        .chunks_exact(2)
                        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]) >> 6)
                        .collect()
                };
                let left: u32 = samples
                    .chunks_exact(16)
                    .flat_map(|row| &row[..8])
                    .map(|&v| v as u32)
                    .sum();
                let right: u32 = samples
                    .chunks_exact(16)
                    .flat_map(|row| &row[8..])
                    .map(|&v| v as u32)
                    .sum();
                assert_eq!(left, black as u32 * 64, "{format:?} color={color}");
                assert!(
                    right > left,
                    "white must stay brighter: {format:?} color={color}"
                );

                // The pre-fix ramp was exactly reversed. The same mapper and
                // renderer must now differ only through glyph coverage.
                let legacy = AsciiConfig {
                    charset: "@%#*+=-:. ".into(),
                    ..config.clone()
                };
                assert_eq!(
                    config.charset.chars().rev().collect::<String>(),
                    legacy.charset
                );
                let legacy_frame = CpuAsciiBackend::new()
                    .process(input.clone(), &legacy)
                    .unwrap()
                    .frame;
                let (legacy_y, _) = legacy_frame.host().planes(&desc);
                let legacy_black: u32 = if format == PixelFormat::Nv12 {
                    legacy_y
                        .chunks_exact(16)
                        .flat_map(|row| &row[..8])
                        .map(|&v| u32::from(v))
                        .sum()
                } else {
                    legacy_y
                        .chunks_exact(32)
                        .flat_map(|row| row[..16].chunks_exact(2))
                        .map(|bytes| u32::from(u16::from_le_bytes([bytes[0], bytes[1]]) >> 6))
                        .sum()
                };
                let legacy_white: u32 = if format == PixelFormat::Nv12 {
                    legacy_y
                        .chunks_exact(16)
                        .flat_map(|row| &row[8..])
                        .map(|&v| u32::from(v))
                        .sum()
                } else {
                    legacy_y
                        .chunks_exact(32)
                        .flat_map(|row| row[16..].chunks_exact(2))
                        .map(|bytes| u32::from(u16::from_le_bytes([bytes[0], bytes[1]]) >> 6))
                        .sum()
                };
                if color {
                    assert_eq!(left, legacy_black, "black changed: {format:?}");
                } else {
                    assert!(
                        left < legacy_black,
                        "dark polarity did not change: {format:?}"
                    );
                }
                assert!(
                    right > legacy_white,
                    "polarity did not change: {format:?} color={color}"
                );
            }
        }
    }
}
