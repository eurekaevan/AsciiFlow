//! Initialization diagnostics: cargo run -p asciiflow-font --example atlas -- FONT OUTPUT.pgm
use asciiflow_font::build_font_atlas;
use std::{io::Write, path::Path};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().collect();
    if arguments.len() != 3 {
        return Err("expected FONT OUTPUT.pgm".into());
    }
    let (atlas, diagnostics) =
        build_font_atlas(Path::new(&arguments[1]), 0, "@%#*+=-:. Ag_", 24, 24)?;
    let mut output = std::fs::File::create(&arguments[2])?;
    writeln!(
        output,
        "P5\n{} {}\n255",
        atlas.width() as usize * atlas.glyph_count(),
        atlas.height()
    )?;
    for row in 0..atlas.height() as usize {
        for glyph in 0..atlas.glyph_count() {
            output.write_all(
                &atlas.glyph(glyph)
                    [row * atlas.width() as usize..(row + 1) * atlas.width() as usize],
            )?;
        }
    }
    println!("{diagnostics:?}; atlas bytes {}", atlas.as_r8_slice().len());
    // Raw glyph-major bytes permit a reproducible hash without a production hash dependency.
    std::fs::write(format!("{}.r8", arguments[2]), atlas.as_r8_slice())?;
    Ok(())
}
