//! C-2B deterministic qualification, consuming the preserved C-1 byte oracle.
use asciiflow_core::tone_map_bt2446::{MethodAOutput, SdrBt2020NonlinearRgb, SdrBt2020Ycbcr};
use asciiflow_cpu::target_volume::convert_frame;
use asciiflow_cpu::tone_map::{SdrBt2020ReferenceFrame, ToneMapDiagnostics};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::time::Instant;

fn read_c1(path: &Path) -> Result<SdrBt2020ReferenceFrame, Box<dyn std::error::Error>> {
    let mut input = BufReader::new(File::open(path)?);
    let mut magic = [0; 13];
    input.read_exact(&mut magic)?;
    if &magic != b"AF-C1-OUT-v1\0" {
        return Err("invalid C-1 magic".into());
    }
    let mut dimensions = [0; 8];
    input.read_exact(&mut dimensions)?;
    let width = u32::from_le_bytes(dimensions[..4].try_into()?);
    let height = u32::from_le_bytes(dimensions[4..].try_into()?);
    if (width, height) != (1920, 1080) {
        return Err("expected canonical C-1 1920x1080".into());
    }
    let count = (width as usize)
        .checked_mul(height as usize)
        .ok_or("pixel count overflow")?;
    let expected = 21 + (count as u64) * 56;
    if input.get_ref().metadata()?.len() != expected {
        return Err("invalid C-1 byte size".into());
    }
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(count)?;
    for _ in 0..count {
        let mut bytes = [0; 56];
        input.read_exact(&mut bytes)?;
        let values: [f64; 7] = std::array::from_fn(|i| {
            f64::from_le_bytes(bytes[8 * i..8 * i + 8].try_into().expect("eight bytes"))
        });
        if !values.iter().all(|v| v.is_finite()) {
            return Err("nonfinite C-1 artifact".into());
        }
        pixels.push(MethodAOutput {
            rgb: SdrBt2020NonlinearRgb {
                r: values[0],
                g: values[1],
                b: values[2],
            },
            ycbcr: SdrBt2020Ycbcr {
                y: values[3],
                cb: values[4],
                cr: values[5],
            },
            mapped_luma: values[6],
        });
    }
    Ok(SdrBt2020ReferenceFrame {
        width,
        height,
        pixels,
        diagnostics: ToneMapDiagnostics::default(),
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: qualify_target_volume C1_INPUT.bin OUTPUT_DIR".into());
    }
    let frame = read_c1(Path::new(&args[0]))?;
    let directory = Path::new(&args[1]);
    fs::create_dir_all(directory)?;
    for run in 1..=3 {
        let start = Instant::now();
        let converted = convert_frame(&frame)?;
        let elapsed = start.elapsed().as_secs_f64();
        // Never truncate a retained artifact, including source aliases via
        // symlinks or hardlinks. Requalification requires a fresh directory.
        let mut output = BufWriter::new(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(directory.join(format!("target-volume-run{run}.bin")))?,
        );
        output.write_all(b"AF-C2B-OUT-v1\0")?;
        output.write_all(&frame.width.to_le_bytes())?;
        output.write_all(&frame.height.to_le_bytes())?;
        for pixel in converted.pixels {
            for value in pixel
                .source_linear
                .components()
                .into_iter()
                .chain(pixel.unbounded.components())
                .chain(pixel.bounded.components())
                .chain(pixel.nonlinear.components())
            {
                output.write_all(&value.to_le_bytes())?;
            }
        }
        output.flush()?;
        println!(
            "run={run} ms_per_frame={:.6} fps={:.6}",
            elapsed * 1000.0,
            1.0 / elapsed
        );
    }
    Ok(())
}
