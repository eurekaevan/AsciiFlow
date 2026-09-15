use crate::{Error, Result};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProcessingBackend {
    #[default]
    Auto,
    Cpu,
    Vulkan,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AsciiConfig {
    pub grid_width: u32,
    pub grid_height: Option<u32>,
    pub charset: String,
    pub font: String,
    pub color: bool,
}

impl Default for AsciiConfig {
    fn default() -> Self {
        Self {
            grid_width: 160,
            grid_height: None,
            charset: "@%#*+=-:. ".into(),
            font: "builtin-8x8".into(),
            color: true,
        }
    }
}

impl AsciiConfig {
    pub fn validate(&self) -> Result<()> {
        if self.grid_width == 0 || self.grid_width > 8192 {
            return Err(Error::InvalidConfig("width must be in 1..=8192".into()));
        }
        if matches!(self.grid_height, Some(0 | 8193..)) {
            return Err(Error::InvalidConfig("height must be in 1..=8192".into()));
        }
        if self.charset.is_empty() {
            return Err(Error::InvalidConfig(
                "charset must contain at least one glyph".into(),
            ));
        }
        if self.charset.chars().count() > u16::MAX as usize {
            return Err(Error::InvalidConfig(
                "charset contains too many glyphs".into(),
            ));
        }
        Ok(())
    }

    pub fn resolved_grid(&self, frame_width: u32, frame_height: u32) -> Result<(u32, u32)> {
        self.validate()?;
        if frame_width == 0 || frame_height == 0 {
            return Err(Error::InvalidConfig(
                "frame dimensions must be non-zero".into(),
            ));
        }
        let width = self.grid_width.min(frame_width);
        let derived = ((width as u64 * frame_height as u64 + frame_width as u64 / 2)
            / frame_width as u64) as u32;
        let height = self.grid_height.unwrap_or(derived.max(1)).min(frame_height);
        Ok((width, height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_width_boundaries_are_explicit_and_overwide_grids_are_clamped() {
        let mut config = AsciiConfig {
            grid_width: 1,
            ..AsciiConfig::default()
        };
        assert_eq!(config.resolved_grid(1920, 1080).unwrap(), (1, 1));
        config.grid_width = 4096;
        assert_eq!(config.resolved_grid(1920, 1080).unwrap().0, 1920);
        config.grid_width = 0;
        assert!(config.validate().is_err());
        config.grid_width = 8193;
        assert!(config.validate().is_err());
    }
}
