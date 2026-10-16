//! Native interface fonts, independent of the font used to render video.
use anyhow::{Context, bail};
use eframe::egui;
use std::sync::Arc;

pub(super) fn install_ui_font(ctx: &egui::Context) -> anyhow::Result<()> {
    let mut fonts = egui::FontDefinitions::default();
    let system = system_ui_font().and_then(|(resolved, size)| {
        insert_font(&mut fonts, "system-ui", resolved)?;
        // Text fields and numeric controls must follow the same interface font too.
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .insert(0, "system-ui".into());
        }
        Ok(size)
    });
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        // Preserve non-Latin file names even when the configured UI face is Latin-only.
        #[cfg(target_os = "linux")]
        let fallback = asciiflow_font::resolve_system_pattern("sans-serif:charset=4e2d 6587");
        #[cfg(target_os = "windows")]
        let fallback = asciiflow_font::resolve_system_ui_font("Microsoft YaHei")
            .or_else(|_| asciiflow_font::resolve_system_ui_font("SimSun"));
        match fallback
            .map_err(anyhow::Error::from)
            .and_then(|font| insert_font(&mut fonts, "system-cjk", font))
        {
            Ok(()) => {
                for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                    fonts
                        .families
                        .entry(family)
                        .or_default()
                        .push("system-cjk".into());
                }
            }
            Err(error) => eprintln!("System CJK interface font unavailable: {error:#}"),
        }
    }
    ctx.set_fonts(fonts);
    let size = system?;
    ctx.style_mut(|style| {
        // Keep the existing hierarchy while matching the system's body text size.
        let previous = style.text_styles[&egui::TextStyle::Body].size;
        for font in style.text_styles.values_mut() {
            font.size *= size / previous;
        }
    });
    Ok(())
}

fn insert_font(
    fonts: &mut egui::FontDefinitions,
    name: &str,
    resolved: asciiflow_font::ResolvedFont,
) -> anyhow::Result<()> {
    let path = resolved
        .path
        .context("System font resolver returned no file")?;
    let mut data = egui::FontData::from_owned(
        std::fs::read(&path).with_context(|| format!("Read UI font {}", path.display()))?,
    );
    // FreeType places a named variable instance in the upper bits; egui uses default axes.
    data.index = u32::try_from(resolved.face_index)? & 0xffff;
    fonts.font_data.insert(name.into(), Arc::new(data));
    Ok(())
}

#[cfg(target_os = "linux")]
fn system_ui_font() -> anyhow::Result<(asciiflow_font::ResolvedFont, f32)> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let description = if desktop.contains("kde") {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
            })
            .context("Cannot locate desktop configuration")?;
        let contents = std::fs::read_to_string(config.join("kdeglobals"))
            .context("Read KDE interface font")?;
        let value = ini_value(&contents, "General", "font")
            .context("KDE interface font is not configured")?;
        let mut fields = value.split(',');
        format!(
            "{} {}",
            fields.next().context("Missing KDE font family")?,
            fields.next().context("Missing KDE font size")?
        )
    } else {
        let output = std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "font-name"])
            .output()
            .context("Read desktop interface font with gsettings")?;
        if !output.status.success() {
            bail!(
                "Read desktop interface font: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let value = String::from_utf8(output.stdout)?;
        value
            .trim()
            .trim_matches('\'')
            .replace("\\'", "'")
            .replace("\\\\", "\\")
    };
    let (family, size) = parse_font_description(&description)?;
    Ok((asciiflow_font::resolve_system_pattern(&family)?, size))
}

#[cfg(target_os = "linux")]
fn ini_value<'a>(contents: &'a str, section: &str, key: &str) -> Option<&'a str> {
    let mut active = false;
    for line in contents.lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            active = &line[1..line.len() - 1] == section;
        } else if active
            && let Some((name, value)) = line.split_once('=')
            && name.trim() == key
        {
            return Some(value.trim());
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn parse_font_description(description: &str) -> anyhow::Result<(String, f32)> {
    let (family, size) = description
        .trim()
        .rsplit_once(' ')
        .context("Desktop font is missing its size")?;
    let pixels = size.ends_with("px");
    let size: f32 = size
        .trim_end_matches("px")
        .parse()
        .context("Invalid desktop font size")?;
    if family.trim().is_empty() || !size.is_finite() || size <= 0.0 {
        bail!("Invalid desktop font description: {description}");
    }
    let mut parts: Vec<_> = family.split_whitespace().collect();
    let mut styles = Vec::new();
    while parts.len() > 1
        && parts.last().is_some_and(|word| {
            matches!(
                *word,
                "Bold"
                    | "Italic"
                    | "Oblique"
                    | "Regular"
                    | "Medium"
                    | "Light"
                    | "Semibold"
                    | "Heavy"
                    | "Thin"
                    | "Condensed"
            )
        })
    {
        styles.insert(0, parts.pop().unwrap());
    }
    // Escape Fontconfig's pattern delimiters in the configured family name.
    let family = parts
        .join(" ")
        .replace('\\', "\\\\")
        .replace('-', "\\-")
        .replace(':', "\\:")
        .replace(',', "\\,");
    let pattern = if styles.is_empty() {
        family
    } else {
        format!("{family}:style={}", styles.join(" "))
    };
    Ok((pattern, if pixels { size } else { size * 96.0 / 72.0 }))
}

#[cfg(target_os = "windows")]
fn system_ui_font() -> anyhow::Result<(asciiflow_font::ResolvedFont, f32)> {
    // Mirrors the Windows SDK LOGFONTW and NONCLIENTMETRICSW layouts.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct LogFont {
        height: i32,
        width: i32,
        escapement: i32,
        orientation: i32,
        weight: i32,
        italic: u8,
        underline: u8,
        strike_out: u8,
        charset: u8,
        out_precision: u8,
        clip_precision: u8,
        quality: u8,
        pitch_and_family: u8,
        face_name: [u16; 32],
    }
    #[repr(C)]
    struct Metrics {
        size: u32,
        border_width: i32,
        scroll_width: i32,
        scroll_height: i32,
        caption_width: i32,
        caption_height: i32,
        caption_font: LogFont,
        small_caption_width: i32,
        small_caption_height: i32,
        small_caption_font: LogFont,
        menu_width: i32,
        menu_height: i32,
        menu_font: LogFont,
        status_font: LogFont,
        message_font: LogFont,
        padded_border_width: i32,
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SystemParametersInfoForDpi(
            action: u32,
            parameter: u32,
            data: *mut std::ffi::c_void,
            flags: u32,
            dpi: u32,
        ) -> i32;
    }
    let mut metrics: Metrics = unsafe { std::mem::zeroed() };
    metrics.size = std::mem::size_of::<Metrics>() as u32;
    // Query at 96 DPI: eframe applies the actual monitor scaling independently.
    if unsafe {
        SystemParametersInfoForDpi(
            0x29,
            metrics.size,
            (&mut metrics as *mut Metrics).cast(),
            0,
            96,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error()).context("Read Windows interface font");
    }
    let font = metrics.message_font;
    let end = font
        .face_name
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(font.face_name.len());
    let family = String::from_utf16(&font.face_name[..end])?;
    let size = font.height.unsigned_abs() as f32;
    if size == 0.0 {
        bail!("Windows returned a zero-sized interface font");
    }
    Ok((asciiflow_font::resolve_system_ui_font(&family)?, size))
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn system_ui_font() -> anyhow::Result<(asciiflow_font::ResolvedFont, f32)> {
    bail!("Native interface font discovery is supported on Linux and Windows");
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn desktop_font_size_and_style_follow_configuration() {
        assert_eq!(
            parse_font_description("Inter Variable 11").unwrap(),
            ("Inter Variable".into(), 11.0 * 96.0 / 72.0)
        );
        assert_eq!(
            parse_font_description("Noto Sans Bold Italic 16px").unwrap(),
            ("Noto Sans:style=Bold Italic".into(), 16.0)
        );
        assert_eq!(parse_font_description("A-B 10").unwrap().0, "A\\-B");
        for value in ["", "Sans", "Sans 0", "Sans -1", "Sans NaN", "Sans inf"] {
            assert!(parse_font_description(value).is_err(), "{value}");
        }
        assert_eq!(
            ini_value(
                "[Other]\nfont=Wrong,10\n[General]\nfont=Inter,11\n",
                "General",
                "font"
            ),
            Some("Inter,11")
        );
    }

    #[test]
    #[ignore = "requires runtime Fontconfig and an installed outline font"]
    fn escaped_ui_families_are_resolved_as_fontconfig_patterns() {
        for description in ["A-B 11", "A:B 11", "A,B 11"] {
            let (pattern, _) = parse_font_description(description).unwrap();
            // Fontconfig may select a substitute for these uninstalled names, but
            // an escaped family must never be mistaken for a relative file path.
            let resolved = asciiflow_font::resolve_system_pattern(&pattern).unwrap();
            assert_eq!(resolved.source, asciiflow_font::FontSource::Fontconfig);
            assert!(resolved.path.unwrap().is_file());
        }
    }

    #[test]
    #[ignore = "requires a desktop font setting and installed interface/CJK fonts"]
    fn desktop_system_font_displays_chinese_file_names() {
        let ctx = egui::Context::default();
        let (_, expected_size) = system_ui_font().unwrap();
        install_ui_font(&ctx).unwrap();
        assert!(
            (ctx.style().text_styles[&egui::TextStyle::Body].size - expected_size).abs() < 0.00001
        );
        let _ = ctx.run(Default::default(), |ctx| {
            ctx.fonts(|fonts| {
                for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                    let font = egui::FontId::new(16.0, family);
                    for character in "中文測試视频.mp4".chars() {
                        assert!(fonts.has_glyph(&font, character), "missing {character}");
                    }
                }
            });
        });
    }
}
