//! A bounded RGB view of the real first-frame CPU renderer, before encoding.
use crate::{ensure_not_cancelled, prepare_font};
use anyhow::{Context, Result, bail, ensure};
use asciiflow_core::{
    AsciiBackend, AsciiConfig, CancellationToken, ColorSpace, FrameSource, PixelFormat,
};
use asciiflow_cpu::CpuAsciiBackend;
use asciiflow_media::Decoder;
use std::path::Path;

#[derive(Debug)]
pub struct Preview {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
    pub description: String,
}

/// Decode and render at source resolution, then bound the display image to 1024px.
/// CPU preview admits only the same 8-bit BT.709 SDR frames as production CPU.
pub fn preview(
    input: &Path,
    config: &AsciiConfig,
    cancellation: CancellationToken,
    font_face_index: Option<u32>,
) -> Result<Preview> {
    ensure_not_cancelled(&cancellation)?;
    ensure!(
        std::fs::metadata(input)
            .context("inspect preview input")?
            .is_file(),
        "Preview input must be a regular file"
    );
    let mut config = config.clone();
    config.charset = crate::args::resolve_charset(&config.charset);
    config.validate()?;
    let mut decoder = Decoder::open(input).context("open preview input")?;
    let frame = decoder
        .next_frame()?
        .context("input contains no video frame")?;
    let desc = frame.desc().clone();
    if desc.format != PixelFormat::Nv12 || desc.color_space != ColorSpace::default() {
        bail!(
            "First-frame preview supports 8-bit BT.709 SDR only; HDR and 10-bit conversion retain their existing hardware requirements"
        );
    }
    ensure_not_cancelled(&cancellation)?;
    let (atlas, font, _) = prepare_font(
        &mut config,
        &desc,
        font_face_index,
        false,
        &mut serde_json::json!({}),
    )?;
    let output = CpuAsciiBackend::with_atlas(atlas, &config)
        .process(frame, &config)?
        .frame;
    ensure_not_cancelled(&cancellation)?;
    let longest = desc.width.max(desc.height);
    let size = |value: u32| {
        ((u64::from(value) * u64::from(longest.min(1024))) / u64::from(longest)).max(1) as u32
    };
    let width = size(desc.width);
    let height = size(desc.height);
    let mut rgb = vec![0; width as usize * height as usize * 3];
    let (y_plane, uv_plane) = output.host().planes(&desc);
    // Limited-range BT.709 Y'CbCr -> display R'G'B'. Sample the rendered pixels
    // directly: tightly packed HostFrame planes have no native SIMD padding.
    for row in 0..height as usize {
        ensure_not_cancelled(&cancellation)?;
        let source_y = row * desc.height as usize / height as usize;
        for column in 0..width as usize {
            let source_x = column * desc.width as usize / width as usize;
            let uv = source_y / 2 * desc.width as usize + (source_x / 2) * 2;
            let y = (f32::from(y_plane[source_y * desc.width as usize + source_x]) - 16.0) / 219.0;
            let cb = (f32::from(uv_plane[uv]) - 128.0) / 224.0;
            let cr = (f32::from(uv_plane[uv + 1]) - 128.0) / 224.0;
            let pixel = [
                y + 1.5748 * cr,
                y - 0.187324 * cb - 0.468124 * cr,
                y + 1.8556 * cb,
            ];
            for (target, value) in rgb[(row * width as usize + column) * 3..][..3]
                .iter_mut()
                .zip(pixel)
            {
                *target = (value * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    ensure_not_cancelled(&cancellation)?;
    let info = decoder.info();
    Ok(Preview {
        width,
        height,
        rgb,
        description: format!(
            "{} · {} × {} · {:.2} fps · {font} · CPU SDR preview",
            info.requirements.codec,
            desc.width,
            desc.height,
            f64::from(info.frame_rate.numerator) / f64::from(info.frame_rate.denominator),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/media/no-audio.mp4")
    }

    #[test]
    fn preview_uses_renderer_and_matches_settings_without_creating_output() {
        let config = AsciiConfig {
            grid_width: 16,
            charset: " ".into(),
            ..Default::default()
        };
        let black = preview(&fixture(), &config, CancellationToken::new(), None).unwrap();
        assert_eq!((black.width, black.height), (64, 64));
        assert_eq!(black.rgb.len(), 64 * 64 * 3);
        assert!(black.rgb.iter().all(|value| *value <= 1));
        let rendered = preview(
            &fixture(),
            &AsciiConfig {
                grid_width: 16,
                ..Default::default()
            },
            CancellationToken::new(),
            None,
        )
        .unwrap();
        assert_ne!(black.rgb, rendered.rgb);
    }

    #[test]
    fn preview_resolves_reserved_presets_identically_to_conversion() {
        for name in ["standard", "detailed"] {
            let config = AsciiConfig {
                grid_width: 16,
                charset: name.into(),
                ..Default::default()
            };
            let preset = preview(&fixture(), &config, CancellationToken::new(), None).unwrap();
            let literal = preview(
                &fixture(),
                &AsciiConfig {
                    charset: crate::args::resolve_charset(name),
                    ..config
                },
                CancellationToken::new(),
                None,
            )
            .unwrap();
            assert_eq!(preset.rgb, literal.rgb, "preset {name}");
        }
    }

    #[test]
    fn preview_uses_the_selected_font_face() {
        let config = AsciiConfig {
            grid_width: 4,
            font: Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/fonts/Inconsolata-Regular.ttf")
                .to_str()
                .unwrap()
                .into(),
            ..Default::default()
        };
        let rendered = preview(&fixture(), &config, CancellationToken::new(), Some(0)).unwrap();
        assert!(rendered.description.contains("Inconsolata"));
        let error = preview(&fixture(), &config, CancellationToken::new(), Some(99)).unwrap_err();
        assert!(format!("{error:#}").contains("face 99"));
    }

    #[test]
    fn preview_reports_cancelled_invalid_input_and_invalid_font() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(
            preview(Path::new("missing"), &AsciiConfig::default(), cancel, None)
                .unwrap_err()
                .chain()
                .any(|e| e
                    .downcast_ref::<asciiflow_core::Error>()
                    .is_some_and(asciiflow_core::Error::is_cancelled))
        );
        assert!(
            preview(
                Path::new("."),
                &AsciiConfig::default(),
                CancellationToken::new(),
                None
            )
            .unwrap_err()
            .to_string()
            .contains("regular file")
        );
        assert!(
            preview(
                &fixture(),
                &AsciiConfig {
                    font: "/missing/font.ttf".into(),
                    ..Default::default()
                },
                CancellationToken::new(),
                None
            )
            .is_err()
        );
    }
}
