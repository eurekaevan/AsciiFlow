//! Deterministic, non-production C-1 qualification fixture and byte oracle.
use asciiflow_core::tone_map_bt2446::LinearBt2020RgbNits as Rgb;
use asciiflow_cpu::tone_map::map_linear_frame;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::Instant;

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;

fn source(x: u32, y: u32) -> Rgb {
    let patches = [
        [0.0; 3],
        [0.0001; 3],
        [100.0; 3],
        [203.0; 3],
        [400.0; 3],
        [1000.0; 3],
        [1000.0, 0.0, 0.0],
        [0.0, 1000.0, 0.0],
        [0.0, 0.0, 1000.0],
        [0.0, 1000.0, 1000.0],
        [1000.0, 0.0, 1000.0],
        [1000.0, 1000.0, 0.0],
        [203.0, 120.0, 80.0],
        [400.0, 200.0, 100.0],
        [100.0, 70.0, 50.0],
        [1.0; 3],
    ];
    let [r, g, b] = if y < HEIGHT / 2 {
        patches[(x * 16 / WIDTH) as usize]
    } else if y < HEIGHT * 2 / 3 {
        [1000.0 * f64::from(x) / f64::from(WIDTH - 1); 3]
    } else {
        // Integer coordinates, no clock/randomness/external media. True linear
        // f64 values, not an 8-bit source shifted into a wider representation.
        [
            1000.0 * f64::from(x) / f64::from(WIDTH - 1),
            1000.0 * f64::from(y - HEIGHT / 2) / f64::from(HEIGHT / 2 - 1),
            1000.0 * f64::from((x + 3 * y) % 1024) / 1023.0,
        ]
    };
    Rgb { r, g, b }
}

fn writer(path: &Path, magic: &[u8]) -> std::io::Result<BufWriter<File>> {
    let mut output = BufWriter::new(File::create(path)?);
    output.write_all(magic)?;
    output.write_all(&WIDTH.to_le_bytes())?;
    output.write_all(&HEIGHT.to_le_bytes())?;
    Ok(output)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args_os()
        .nth(1)
        .ok_or("usage: qualify_tone_map OUTPUT_DIR")?;
    let directory = Path::new(&directory);
    fs::create_dir_all(directory)?;
    let input: Vec<_> = (0..HEIGHT)
        .flat_map(|y| (0..WIDTH).map(move |x| source(x, y)))
        .collect();
    let mut serialized = writer(
        &directory.join("linear-bt2020-1000-v1.bin"),
        b"AF-C1-IN-v1\0",
    )?;
    for p in &input {
        for v in [p.r, p.g, p.b] {
            serialized.write_all(&v.to_le_bytes())?;
        }
    }
    serialized.flush()?;
    for run in 1..=3 {
        let start = Instant::now();
        let frame = map_linear_frame(WIDTH, HEIGHT, &input)?;
        let elapsed = start.elapsed();
        let mut output = writer(
            &directory.join(format!("method-a-run{run}.bin")),
            b"AF-C1-OUT-v1\0",
        )?;
        for p in &frame.pixels {
            // Explicit field order, f64 LE; never serialize native struct padding.
            for v in [
                p.rgb.r,
                p.rgb.g,
                p.rgb.b,
                p.ycbcr.y,
                p.ycbcr.cb,
                p.ycbcr.cr,
                p.mapped_luma,
            ] {
                output.write_all(&v.to_le_bytes())?;
            }
        }
        output.flush()?;
        println!(
            "run={run} ms_per_frame={:.6} fps={:.6} diagnostics={:?}",
            elapsed.as_secs_f64() * 1000.0,
            1.0 / elapsed.as_secs_f64(),
            frame.diagnostics
        );
    }
    Ok(())
}
