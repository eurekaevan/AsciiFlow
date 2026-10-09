use crate::{Error, Result};

/// Sparse to dense: luma increases toward glyphs with more ink on black.
pub const STANDARD_CHARSET: &str = " .:-=+*#%@";

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
            charset: STANDARD_CHARSET.into(),
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
        self.resolved_grid_with_aspect(frame_width, frame_height, 1, 1)
    }

    /// Integer, half-up automatic rows; explicit height ignores the font aspect.
    pub fn resolved_grid_with_aspect(
        &self,
        frame_width: u32,
        frame_height: u32,
        cell_width: u32,
        cell_height: u32,
    ) -> Result<(u32, u32)> {
        self.validate()?;
        if frame_width == 0 || frame_height == 0 {
            return Err(Error::InvalidConfig(
                "frame dimensions must be non-zero".into(),
            ));
        }
        let width = self.grid_width.min(frame_width);
        if let Some(height) = self.grid_height {
            return Ok((width, height.min(frame_height)));
        }
        if cell_width == 0 || cell_height == 0 {
            return Err(Error::InvalidConfig(
                "font cell dimensions must be non-zero".into(),
            ));
        }
        // u128 covers all u32 dimensions, including adversarial aspect ratios.
        let numerator = u128::from(width) * u128::from(frame_height) * u128::from(cell_width);
        let denominator = u128::from(frame_width) * u128::from(cell_height);
        let height =
            ((numerator + denominator / 2) / denominator).clamp(1, u128::from(frame_height)) as u32;
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

    #[test]
    fn font_aspect_changes_only_automatic_rows_with_single_integer_rounding() {
        let mut config = AsciiConfig::default();
        assert_eq!(config.resolved_grid(1920, 1080).unwrap(), (160, 90));
        assert_eq!(
            config.resolved_grid_with_aspect(1920, 1080, 8, 8).unwrap(),
            (160, 90)
        );
        assert_eq!(
            config
                .resolved_grid_with_aspect(1920, 1080, 600, 1000)
                .unwrap(),
            (160, 54)
        );
        config.grid_height = Some(90);
        assert_eq!(
            config
                .resolved_grid_with_aspect(1920, 1080, 600, 1000)
                .unwrap(),
            (160, 90)
        );
        assert_eq!(
            config.resolved_grid_with_aspect(1920, 1080, 0, 0).unwrap(),
            (160, 90)
        );
        config.grid_height = None;
        assert!(config.resolved_grid_with_aspect(1920, 1080, 1, 0).is_err());
        assert_eq!(
            config
                .resolved_grid_with_aspect(1920, 1080, 1, u32::MAX)
                .unwrap(),
            (160, 1)
        );
        assert_eq!(
            config
                .resolved_grid_with_aspect(u32::MAX, u32::MAX, u32::MAX, 1)
                .unwrap(),
            (160, u32::MAX)
        );
        config.grid_width = 1;
        assert_eq!(
            config.resolved_grid_with_aspect(2, 3, 1, 3).unwrap(),
            (1, 1)
        );
    }
}
