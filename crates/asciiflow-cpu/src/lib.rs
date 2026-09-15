mod mapper;
mod renderer;

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
