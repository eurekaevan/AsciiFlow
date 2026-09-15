use font8x8::{BASIC_FONTS, UnicodeFonts};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FontError {
    #[error("the built-in font does not contain glyph {0:?}")]
    MissingGlyph(char),
    #[error("only the built-in font is available in Stage 0; got {0:?}")]
    UnsupportedFont(String),
}

#[derive(Clone, Debug)]
pub struct GlyphAtlas {
    width: u32,
    height: u32,
    glyphs: Vec<u8>,
}

impl GlyphAtlas {
    pub fn builtin(font: &str, charset: &str) -> Result<Self, FontError> {
        if font != "builtin-8x8" {
            return Err(FontError::UnsupportedFont(font.into()));
        }
        let mut glyphs = Vec::with_capacity(charset.chars().count() * 64);
        for character in charset.chars() {
            let bitmap = BASIC_FONTS
                .get(character)
                .ok_or(FontError::MissingGlyph(character))?;
            for row in bitmap {
                for x in 0..8 {
                    glyphs.push(if row & (1 << x) == 0 { 0 } else { 255 });
                }
            }
        }
        Ok(Self {
            width: 8,
            height: 8,
            glyphs,
        })
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn glyph_count(&self) -> usize {
        self.glyphs.len() / (self.width * self.height) as usize
    }
    pub fn as_r8_slice(&self) -> &[u8] {
        &self.glyphs
    }
    pub fn glyph(&self, index: usize) -> &[u8] {
        let len = (self.width * self.height) as usize;
        &self.glyphs[index * len..(index + 1) * len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atlas_is_r8_and_stable() {
        let atlas = GlyphAtlas::builtin("builtin-8x8", "@ ").unwrap();
        assert_eq!(atlas.glyph(0).len(), 64);
        assert!(atlas.glyph(1).iter().all(|&v| v == 0));
    }
}
