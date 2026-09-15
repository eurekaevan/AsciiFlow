pub fn glyph_lookup_table(glyph_count: usize) -> [u32; 256] {
    assert!(glyph_count > 0 && glyph_count <= u16::MAX as usize);
    std::array::from_fn(|value| {
        let normalized = (value as u8).saturating_sub(16) as f32 / 219.0;
        let clamped = normalized.clamp(0.0, 1.0);
        let curved = clamped * clamped * (3.0 - 2.0 * clamped);
        (curved * (glyph_count - 1) as f32).round() as u32
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limited_range_endpoints_map_to_charset_endpoints() {
        let lut = glyph_lookup_table(10);
        assert_eq!(lut[0], 0);
        assert_eq!(lut[16], 0);
        assert_eq!(lut[235], 9);
        assert_eq!(lut[255], 9);
    }
}
