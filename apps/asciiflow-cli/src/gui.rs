//! Native, single-task interface. All media and GPU ownership stays on the worker.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use asciiflow_cli::{Preview, args::Args, preview, run_with_progress};
use asciiflow_core::{AsciiConfig, CancellationToken};
use clap::Parser;
use eframe::egui;
mod gui_system;
use std::{
    ffi::OsString,
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::Instant,
};

fn main() -> std::process::ExitCode {
    if std::env::args_os()
        .skip(1)
        .eq([OsString::from("--version")])
    {
        println!(
            "AsciiFlow GUI {}",
            option_env!("ASCIIFLOW_RELEASE_DATE").unwrap_or("dev")
        );
        return std::process::ExitCode::SUCCESS;
    }
    // Set this process-global option before the window backend starts any threads.
    unsafe { ffmpeg_sys_next::av_log_set_level(ffmpeg_sys_next::AV_LOG_WARNING) };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 760.0])
            .with_min_inner_size([820.0, 600.0])
            .with_icon(application_icon()),
        #[cfg(target_os = "linux")]
        event_loop_builder: Some(Box::new(|builder| {
            use winit::platform::x11::EventLoopBuilderExtX11;
            // winit 0.30 has no Wayland file-drop events. XWayland bridges drops
            // from native file managers without changing process environment.
            if std::env::var_os("DISPLAY").is_some_and(|value| !value.is_empty()) {
                builder.with_x11();
            }
        })),
        ..Default::default()
    };
    if let Err(error) = eframe::run_native(
        "AsciiFlow",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    ) {
        let message = format!("Could not open the AsciiFlow window: {error}");
        eprintln!("{message}");
        rfd::MessageDialog::new()
            .set_title("AsciiFlow startup failed")
            .set_level(rfd::MessageLevel::Error)
            .set_description(&message)
            .show();
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

fn application_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/asciiflow.png"))
        .expect("the embedded application icon must be a valid PNG")
}

#[derive(Clone, PartialEq)]
struct Settings {
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    config: AsciiConfig,
    font_face_index: Option<u32>,
    audio: &'static str,
    backend: &'static str,
    encoder: &'static str,
    codec: &'static str,
    depth: &'static str,
    dynamic_range: &'static str,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            input: None,
            output: None,
            config: AsciiConfig {
                charset: "standard".into(),
                ..Default::default()
            },
            font_face_index: None,
            audio: "auto",
            backend: "auto",
            encoder: "auto",
            codec: "h264",
            depth: "8",
            dynamic_range: "preserve",
        }
    }
}

impl Settings {
    fn args(&self) -> anyhow::Result<Args> {
        let input = self
            .input
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Choose an input video."))?;
        let output = self
            .output
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Choose an output file."))?;
        let mut values: Vec<OsString> = vec!["asciiflow".into()];
        for (flag, value) in [
            ("--backend", self.backend.to_owned()),
            ("--encode", self.encoder.to_owned()),
            ("--audio", self.audio.to_owned()),
            ("--output-codec", self.codec.to_owned()),
            ("--output-bit-depth", self.depth.to_owned()),
            ("--output-dynamic-range", self.dynamic_range.to_owned()),
            ("--width", self.config.grid_width.to_string()),
            ("--charset", self.config.charset.clone()),
            ("--font", self.config.font.clone()),
            ("--color", self.config.color.to_string()),
        ] {
            values.push(format!("{flag}={value}").into());
        }
        if let Some(height) = self.config.grid_height {
            values.extend(["--height".into(), height.to_string().into()]);
        }
        if let Some(index) = self.font_face_index {
            values.extend(["--font-face-index".into(), index.to_string().into()]);
        }
        values.push("--no-progress".into());
        values.extend([
            "--".into(),
            input.as_os_str().into(),
            output.as_os_str().into(),
        ]);
        Ok(Args::try_parse_from(values)?)
    }
}

#[derive(Default)]
struct Progress {
    frames: u64,
    total: Option<u64>,
}

enum Outcome {
    Preview(Preview),
    Converted,
}

struct Job {
    handle: JoinHandle<()>,
    result: mpsc::Receiver<Result<Outcome, String>>,
    cancellation: CancellationToken,
    progress: Arc<Mutex<Progress>>,
    started: Instant,
    conversion: bool,
    settings: Settings,
}

struct App {
    logo: egui::TextureHandle,
    file_drop_supported: bool,
    fonts: Vec<asciiflow_font::SystemFont>,
    font_scan:
        Option<JoinHandle<Result<Vec<asciiflow_font::SystemFont>, asciiflow_font::FontError>>>,
    font_error: Option<String>,
    font_filter: String,
    custom_charset: String,
    settings: Settings,
    job: Option<Job>,
    texture: Option<egui::TextureHandle>,
    preview_settings: Option<Settings>,
    preview_description: String,
    completed_output: Option<PathBuf>,
    location_job: Option<JoinHandle<anyhow::Result<()>>>,
    status: String,
    failed: bool,
    closing: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::from_rgb(20, 26, 35);
        visuals.window_fill = egui::Color32::from_rgb(27, 35, 46);
        visuals.extreme_bg_color = egui::Color32::from_rgb(13, 18, 25);
        visuals.faint_bg_color = egui::Color32::from_rgb(28, 37, 49);
        visuals.override_text_color = Some(egui::Color32::from_rgb(222, 230, 239));
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(36, 47, 61);
        visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(36, 47, 61);
        visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(45, 64, 79);
        visuals.widgets.active.weak_bg_fill = egui::Color32::from_rgb(32, 112, 124);
        visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
        visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);
        visuals.widgets.active.corner_radius = egui::CornerRadius::same(6);
        visuals.selection.bg_fill = egui::Color32::from_rgb(28, 98, 108);
        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(32, 112, 124);
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(45, 64, 79);
        cc.egui_ctx.set_visuals(visuals);
        cc.egui_ctx.style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(10.0, 6.0);
            style.spacing.button_padding = egui::vec2(12.0, 6.0);
            style.spacing.interact_size.y = 32.0;
        });
        if let Err(error) = gui_system::install_ui_font(&cc.egui_ctx) {
            eprintln!("System interface font unavailable: {error:#}");
        }
        #[cfg(target_os = "linux")]
        let file_drop_supported = {
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            !matches!(
                cc.window_handle().map(|handle| handle.as_raw()),
                Ok(RawWindowHandle::Wayland(_))
            )
        };
        #[cfg(not(target_os = "linux"))]
        let file_drop_supported = true;
        let repaint = cc.egui_ctx.clone();
        let font_scan = thread::Builder::new()
            .name("asciiflow-font-scan".into())
            .spawn(move || {
                let fonts = asciiflow_font::list_system_fonts();
                repaint.request_repaint();
                fonts
            });
        let (font_scan, font_error) = match font_scan {
            Ok(handle) => (Some(handle), None),
            Err(error) => (None, Some(format!("Could not scan system fonts: {error}"))),
        };
        let icon = application_icon();
        let logo = cc.egui_ctx.load_texture(
            "asciiflow-icon",
            egui::ColorImage::from_rgba_unmultiplied(
                [icon.width as usize, icon.height as usize],
                &icon.rgba,
            ),
            egui::TextureOptions::LINEAR,
        );
        Self {
            logo,
            fonts: Vec::new(),
            font_scan,
            font_error,
            font_filter: String::new(),
            custom_charset: AsciiConfig::default().charset,
            file_drop_supported,
            settings: Settings::default(),
            job: None,
            texture: None,
            preview_settings: None,
            preview_description: String::new(),
            completed_output: None,
            location_job: None,
            status: "Choose a video to get started.".into(),
            failed: false,
            closing: false,
        }
    }

    fn input(&mut self, path: PathBuf) {
        // A new input needs an explicit new destination; never reuse the previous job's path.
        self.settings.input = Some(path);
        self.settings.output = None;
        self.texture = None;
        self.preview_settings = None;
        self.preview_description.clear();
        self.status = "Ready to preview the first frame.".into();
        self.failed = false;
    }

    fn start(&mut self, ctx: &egui::Context, conversion: bool, frame: &eframe::Frame) {
        if self.job.is_some() {
            return;
        }
        let settings = self.settings.clone();
        let args = if conversion {
            match settings.args() {
                Ok(args) => Some(args),
                Err(error) => {
                    self.error(format!("{error:#}"));
                    return;
                }
            }
        } else {
            None
        };
        let Some(input) = settings.input.clone() else {
            self.error("Choose an input video.".into());
            return;
        };
        if let Err(error) = settings.config.validate() {
            self.error(error.to_string());
            return;
        }
        if conversion && settings.output.as_ref().is_some_and(|path| path.exists()) {
            let response = rfd::MessageDialog::new().set_parent(frame)
                .set_title("Replace existing output?")
                .set_level(rfd::MessageLevel::Warning)
                .set_description("The destination already exists. Replace it only after conversion completes successfully?")
                .set_buttons(rfd::MessageButtons::YesNo)
                .show();
            if response != rfd::MessageDialogResult::Yes {
                return;
            }
        }
        let cancellation = CancellationToken::new();
        let worker_cancellation = cancellation.clone();
        let progress = Arc::new(Mutex::new(Progress::default()));
        let worker_progress = Arc::clone(&progress);
        let config = settings.config.clone();
        let font_face_index = settings.font_face_index;
        let repaint = ctx.clone();
        let (sender, result) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name("asciiflow-task".into())
            .spawn(move || {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if let Some(args) = args {
                        let progress_repaint = repaint.clone();
                        run_with_progress(args, worker_cancellation, move |frames, total| {
                            if let Ok(mut state) = worker_progress.lock() {
                                *state = Progress { frames, total };
                            }
                            progress_repaint.request_repaint();
                        })
                        .map(|()| Outcome::Converted)
                    } else {
                        preview(&input, &config, worker_cancellation, font_face_index)
                            .map(Outcome::Preview)
                    }
                }));
                let outcome = match outcome {
                    Ok(result) => result.map_err(|error| {
                        let cancelled = error.chain().any(|cause| {
                            cause
                                .downcast_ref::<asciiflow_core::Error>()
                                .is_some_and(asciiflow_core::Error::is_cancelled)
                        });
                        if cancelled {
                            format!("Cancelled. {error:#}")
                        } else {
                            format!("{error:#}")
                        }
                    }),
                    Err(payload) => {
                        let detail = payload
                            .downcast_ref::<String>()
                            .map(String::as_str)
                            .or_else(|| payload.downcast_ref::<&str>().copied())
                            .unwrap_or("unknown panic");
                        Err(format!("The processing worker panicked: {detail}"))
                    }
                };
                let _ = sender.send(outcome);
                repaint.request_repaint();
            });
        match handle {
            Ok(handle) => {
                self.job = Some(Job {
                    handle,
                    result,
                    cancellation,
                    progress,
                    started: Instant::now(),
                    conversion,
                    settings,
                });
                self.status = if conversion {
                    "Preparing conversion…"
                } else {
                    "Rendering the first frame…"
                }
                .into();
                self.failed = false;
            }
            Err(error) => self.error(format!("Could not start the processing worker: {error}")),
        }
    }

    fn error(&mut self, message: String) {
        self.status = message;
        self.failed = true;
    }

    fn complete(&mut self, ctx: &egui::Context) {
        if !self
            .job
            .as_ref()
            .is_some_and(|job| job.handle.is_finished())
        {
            return;
        }
        let job = self.job.take().expect("finished job exists");
        // is_finished ensures joining cannot wait for media/GPU work on the UI thread.
        if job.handle.join().is_err() {
            self.error("The processing worker exited unexpectedly.".into());
            return;
        }
        let outcome = job
            .result
            .try_recv()
            .unwrap_or_else(|_| Err("The processing worker returned no result.".into()));
        match outcome {
            Ok(Outcome::Preview(preview)) => {
                if job.settings == self.settings {
                    let image = egui::ColorImage::from_rgb(
                        [preview.width as usize, preview.height as usize],
                        &preview.rgb,
                    );
                    self.texture =
                        Some(ctx.load_texture("preview", image, egui::TextureOptions::NEAREST));
                    self.preview_settings = Some(job.settings);
                    self.preview_description = preview.description;
                    self.status = "First-frame preview ready. Preview uses CPU rendering; output follows your selected processing settings.".into();
                }
            }
            Ok(Outcome::Converted) => {
                self.completed_output = job.settings.output.clone();
                self.status = format!(
                    "Conversion complete in {:.1}s. Saved to {}",
                    job.started.elapsed().as_secs_f64(),
                    job.settings
                        .output
                        .as_ref()
                        .and_then(|path| path.file_name())
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default()
                );
            }
            Err(error) => self.error(error),
        }
    }

    fn files(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        ui.heading("Your video");
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(56.0, 32.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_width(56.0);
                    ui.label(egui::RichText::new("INPUT").small().weak());
                },
            );
            if ui.button("Choose video…").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .set_parent(frame)
                    .add_filter("MP4 video", &["mp4", "mov", "m4v"])
                    .pick_file()
            {
                self.input(path);
            }
        });
        if let Some(path) = &self.settings.input {
            ui.add(
                egui::Label::new(path.file_name().unwrap_or_default().to_string_lossy()).truncate(),
            )
            .on_hover_text(path.display().to_string());
        } else {
            ui.small(if self.file_drop_supported {
                "Or drop a video into this window."
            } else {
                "Choose a video. Dragging is unavailable on native Wayland."
            });
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(egui::vec2(56.0, 32.0),
                egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_width(56.0);
                    ui.label(egui::RichText::new("OUTPUT").small().weak());
                });
            if ui.button("Save output as…")
                .on_hover_text("Existing files require confirmation and are replaced only after successful conversion.").clicked() {
                let name = self.settings.input.as_ref().and_then(|path| path.file_stem())
                    .map(|stem| format!("{}-ascii.mp4", stem.to_string_lossy()))
                    .unwrap_or_else(|| "output-ascii.mp4".into());
                if let Some(path) = rfd::FileDialog::new().set_parent(frame)
                    .add_filter("MP4 video", &["mp4"]).set_file_name(&name).save_file() {
                    self.settings.output = Some(path);
                }
            }
        });
        if let Some(path) = &self.settings.output {
            ui.add(
                egui::Label::new(path.file_name().unwrap_or_default().to_string_lossy()).truncate(),
            )
            .on_hover_text(path.display().to_string());
        } else {
            ui.small("Choose an MP4 destination before converting.");
        }
    }

    fn settings(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        ui.heading("Appearance");
        ui.small("Shape your character canvas");
        ui.add_space(8.0);
        let label = ui.label(egui::RichText::new("Rendering font").strong());
        let selected = if self.settings.config.font == "builtin-8x8" {
            "Built-in · 8 × 8".to_owned()
        } else {
            self.fonts
                .iter()
                .find(|font| {
                    font.path.to_str() == Some(self.settings.config.font.as_str())
                        && Some(font.face_index) == self.settings.font_face_index
                })
                .map(|font| font.name.clone())
                .unwrap_or_else(|| {
                    PathBuf::from(&self.settings.config.font)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                })
        };
        egui::ComboBox::from_id_salt("system-font")
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .truncate()
            .selected_text(selected)
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                ui.set_width(320.0);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                ui.add(
                    egui::TextEdit::singleline(&mut self.font_filter)
                        .hint_text("Search installed fonts…")
                        .desired_width(ui.available_width()),
                );
                ui.separator();
                if ui
                    .selectable_label(
                        self.settings.config.font == "builtin-8x8",
                        "Built-in · 8 × 8",
                    )
                    .clicked()
                {
                    self.settings.config.font = "builtin-8x8".into();
                    self.settings.font_face_index = None;
                    ui.memory_mut(|memory| memory.close_popup());
                }
                let filter = self.font_filter.to_lowercase();
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        let mut matches = 0;
                        for font in &self.fonts {
                            if !font.name.to_lowercase().contains(&filter) {
                                continue;
                            }
                            // Non-Unicode font paths cannot be passed to the renderer's font option.
                            let Some(path) = font.path.to_str() else {
                                continue;
                            };
                            matches += 1;
                            let selected = self.settings.config.font == path
                                && self.settings.font_face_index == Some(font.face_index);
                            if ui
                                .selectable_label(selected, &font.name)
                                .on_hover_text(format!(
                                    "{} · face {}",
                                    font.path.display(),
                                    font.face_index
                                ))
                                .clicked()
                            {
                                self.settings.config.font = path.into();
                                self.settings.font_face_index = Some(font.face_index);
                                ui.memory_mut(|memory| memory.close_popup());
                            }
                        }
                        if matches == 0 && self.font_scan.is_none() {
                            ui.weak("No matching system fonts.");
                        }
                    });
            })
            .response
            .labelled_by(label.id);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);
            if self.font_scan.is_some() {
                ui.spinner();
                ui.small("Scanning installed fonts…");
            } else {
                ui.small(format!("{} installed monospaced faces", self.fonts.len()));
            }
            if ui.small_button("Browse font…").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .set_parent(frame)
                    .add_filter("Font", &["ttf", "otf", "ttc"])
                    .pick_file()
            {
                match path.to_str() {
                    Some(path) => {
                        self.settings.config.font = path.into();
                        self.settings.font_face_index = None;
                    }
                    None => self.error(
                        "This font path cannot be represented as Unicode. Choose another path."
                            .into(),
                    ),
                }
            }
        });
        if let Some(error) = &self.font_error {
            ui.colored_label(egui::Color32::from_rgb(255, 177, 126), error);
        }
        ui.add_space(8.0);
        ui.separator();
        egui::Grid::new("appearance")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                let label = ui.label("Columns");
                ui.add(egui::DragValue::new(&mut self.settings.config.grid_width).range(1..=8192))
                    .labelled_by(label.id);
                ui.end_row();
                let label = ui.label("Rows");
                let mut height = self.settings.config.grid_height.unwrap_or(0);
                if ui
                    .add(
                        egui::DragValue::new(&mut height)
                            .range(0..=8192)
                            .custom_formatter(|value, _| {
                                if value == 0.0 {
                                    "Auto".into()
                                } else {
                                    format!("{value:.0}")
                                }
                            }),
                    )
                    .labelled_by(label.id)
                    .changed()
                {
                    self.settings.config.grid_height = (height != 0).then_some(height);
                }
                ui.end_row();
                ui.label("Color");
                ui.checkbox(&mut self.settings.config.color, "Retain source colors");
                ui.end_row();
                let label = ui.label("Character set");
                let previous = match self.settings.config.charset.as_str() {
                    "standard" => "standard",
                    "detailed" => "detailed",
                    _ => "custom",
                };
                let mut preset = previous;
                let selection = choice(
                    ui,
                    "character-set",
                    &mut preset,
                    &[
                        ("standard", "Standard"),
                        ("detailed", "Detailed"),
                        ("custom", "Custom"),
                    ],
                )
                .labelled_by(label.id);
                if preset != previous {
                    if previous == "custom" {
                        self.custom_charset = self.settings.config.charset.clone();
                    }
                    self.settings.config.charset = if preset == "custom" {
                        self.custom_charset.clone()
                    } else {
                        preset.into()
                    };
                }
                let ramp = asciiflow_cli::args::resolve_charset(&self.settings.config.charset);
                selection.on_hover_text(format!(
                    "{} characters · sparse to dense\n{ramp}",
                    ramp.chars().count()
                ));
                ui.end_row();
                if preset == "custom" {
                    let label = ui.label("Characters");
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.settings.config.charset)
                                .desired_width(210.0),
                        )
                        .labelled_by(label.id)
                        .changed()
                    {
                        self.custom_charset = self.settings.config.charset.clone();
                    }
                    ui.end_row();
                }
                let label = ui.label("Audio");
                choice(
                    ui,
                    "audio",
                    &mut self.settings.audio,
                    &[
                        ("auto", "Auto"),
                        ("copy", "Require passthrough"),
                        ("none", "No audio"),
                    ],
                )
                .labelled_by(label.id);
                ui.end_row();
            });
        ui.small("Rows: Auto keeps the source aspect. Characters run from sparse to dense.");
        ui.add_space(12.0);
        ui.separator();
        ui.collapsing("Advanced processing", |ui| {
            egui::Grid::new("advanced").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
                let label = ui.label("Processing");
                choice(ui, "backend", &mut self.settings.backend, &[("auto", "Auto"), ("cpu", "CPU"), ("vulkan", "Vulkan")]).labelled_by(label.id);
                ui.end_row();
                let label = ui.label("Encoder");
                choice(ui, "encoder", &mut self.settings.encoder, &[("auto", "Auto"), ("vaapi", "VAAPI (Linux)"), ("mf", "Media Foundation (Windows)"), ("software", "Software (custom build)")]).labelled_by(label.id);
                ui.end_row();
                let label = ui.label("Video codec");
                choice(ui, "codec", &mut self.settings.codec, &[("h264", "H.264"), ("hevc", "HEVC"), ("av1", "AV1")]).labelled_by(label.id);
                ui.end_row();
                let label = ui.label("Bit depth");
                choice(ui, "depth", &mut self.settings.depth, &[("8", "8 bit"), ("10", "10 bit")]).labelled_by(label.id);
                ui.end_row();
                let label = ui.label("Dynamic range");
                choice(ui, "range", &mut self.settings.dynamic_range, &[("preserve", "Preserve"), ("sdr", "Convert to SDR")]).labelled_by(label.id);
                ui.end_row();
            });
            ui.small("Official LGPL packages need a supported hardware encoder. HEVC, AV1 and HDR require qualified Linux hardware paths.");
        });
    }
}

fn choice(
    ui: &mut egui::Ui,
    id: &str,
    current: &mut &'static str,
    choices: &[(&'static str, &str)],
) -> egui::Response {
    let selected = choices
        .iter()
        .find(|(value, _)| value == current)
        .map_or(*current, |(_, label)| *label);
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            for &(value, label) in choices {
                ui.selectable_value(current, value, label);
            }
        })
        .response
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.complete(ctx);
        if self
            .location_job
            .as_ref()
            .is_some_and(|job| job.is_finished())
        {
            match self
                .location_job
                .take()
                .expect("finished location request exists")
                .join()
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    self.error(format!("Could not open the file location: {error:#}"))
                }
                Err(_) => self.error("The file location request stopped unexpectedly.".into()),
            }
        }
        if self.location_job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self
            .font_scan
            .as_ref()
            .is_some_and(|scan| scan.is_finished())
        {
            match self
                .font_scan
                .take()
                .expect("finished font scan exists")
                .join()
            {
                Ok(Ok(fonts)) => self.fonts = fonts,
                Ok(Err(error)) => {
                    self.font_error = Some(format!("System font scan failed: {error}"))
                }
                Err(_) => {
                    self.font_error = Some("The system font scan stopped unexpectedly.".into())
                }
            }
        }
        if self.font_scan.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        ctx.accesskit_node_builder(egui::accesskit_root_id(), |node| {
            node.set_label("AsciiFlow")
        });
        if ctx.input(|input| input.viewport().close_requested())
            && let Some(job) = &self.job
        {
            self.closing = true;
            job.cancellation.cancel();
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if self.closing && self.job.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.job.is_none() && !self.closing {
            let dropped = ctx.input(|input| {
                input
                    .raw
                    .dropped_files
                    .iter()
                    .find_map(|file| file.path.clone())
            });
            if let Some(path) = dropped {
                self.input(path);
            }
        }
        egui::TopBottomPanel::top("title")
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(egui::Margin::symmetric(20, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    ui.add(
                        egui::Image::new(&self.logo)
                            .fit_to_exact_size(egui::vec2(40.0, 40.0))
                            .alt_text("AsciiFlow AF icon"),
                    );
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("AsciiFlow").size(22.0).strong());
                        ui.small("Turn motion into characters");
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("LOCAL PROCESSING").small().weak());
                    });
                });
            });
        egui::TopBottomPanel::bottom("task")
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(egui::Margin::symmetric(20, 12)),
            )
            .show(ctx, |ui| {
                if let Some(job) = &self.job {
                    let (frames, total) = job
                        .progress
                        .lock()
                        .map(|state| (state.frames, state.total))
                        .unwrap_or_default();
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(if self.closing {
                            "Cancelling before closing…"
                        } else if job.cancellation.is_cancelled() {
                            "Cancelling…"
                        } else if job.conversion {
                            "Converting…"
                        } else {
                            "Rendering preview…"
                        });
                        if ui
                            .add_enabled(
                                !job.cancellation.is_cancelled(),
                                egui::Button::new("Cancel"),
                            )
                            .clicked()
                        {
                            job.cancellation.cancel();
                        }
                    });
                    if job.conversion {
                        if let Some(total) = total.filter(|total| *total > 0) {
                            ui.add(
                                egui::ProgressBar::new(
                                    (frames as f64 / total as f64).clamp(0.0, 1.0) as f32,
                                )
                                .text(format!("{frames} / {total} frames")),
                            );
                        } else {
                            ui.label(format!("{frames} frames processed · total unknown"));
                        }
                        let seconds = job.started.elapsed().as_secs_f64();
                        if frames > 0 && seconds > 0.0 {
                            ui.small(format!(
                                "{:.1} frames/s · {:.0}s elapsed",
                                frames as f64 / seconds,
                                seconds
                            ));
                        }
                    }
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(10.0, 8.0);
                        let ready = self.settings.input.is_some() && !self.closing;
                        if ui
                            .add_enabled(
                                ready,
                                egui::Button::new("Preview first frame")
                                    .min_size(egui::vec2(160.0, 38.0)),
                            )
                            .clicked()
                        {
                            self.start(ctx, false, frame);
                        }
                        if ui
                            .add_enabled(
                                ready && self.settings.output.is_some(),
                                egui::Button::new(egui::RichText::new("Start conversion").strong())
                                    .min_size(egui::vec2(180.0, 38.0))
                                    .fill(egui::Color32::from_rgb(28, 98, 108)),
                            )
                            .clicked()
                        {
                            self.start(ctx, true, frame);
                        }
                        if let Some(output) = self.completed_output.clone()
                            && ui
                                .add_enabled(
                                    self.location_job.is_none(),
                                    egui::Button::new("Open file location")
                                        .min_size(egui::vec2(160.0, 38.0)),
                                )
                                .on_hover_text(output.display().to_string())
                                .clicked()
                        {
                            match thread::Builder::new()
                                .name("asciiflow-file-location".into())
                                .spawn(move || reveal_output(&output))
                            {
                                Ok(job) => self.location_job = Some(job),
                                Err(error) => self
                                    .error(format!("Could not request the file location: {error}")),
                            }
                        }
                    });
                }
                ui.add_space(4.0);
                ui.label(egui::RichText::new(&self.status).color(if self.failed {
                    egui::Color32::from_rgb(255, 155, 145)
                } else {
                    ui.visuals().text_color()
                }));
            });
        egui::SidePanel::left("controls")
            .resizable(false)
            .exact_width(390.0)
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(16.0),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_enabled_ui(self.job.is_none() && !self.closing, |ui| {
                        egui::Frame::group(ui.style())
                            .fill(ui.visuals().faint_bg_color)
                            .inner_margin(14.0)
                            .corner_radius(10)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                self.files(ui, frame);
                            });
                        ui.add_space(10.0);
                        self.settings(ui, frame);
                    });
                });
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(13, 18, 25))
                    .inner_margin(24.0),
            )
            .show(ctx, |ui| {
                ui.heading("Preview canvas");
                ui.small("CPU preview · 8-bit BT.709 SDR inputs · before output encoding");
                ui.add_space(10.0);
                if let Some(texture) = &self.texture {
                    if self.preview_settings.as_ref() != Some(&self.settings) {
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            "Settings changed — refresh the preview.",
                        );
                    }
                    ui.label(&self.preview_description);
                    let available = ui.available_size();
                    let original = texture.size_vec2();
                    let scale = (available.x / original.x)
                        .min(available.y / original.y)
                        .clamp(0.01, 1.0);
                    ui.centered_and_justified(|ui| {
                        ui.add(egui::Image::new(texture).fit_to_exact_size(original * scale));
                    });
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                        egui::RichText::new(if self.file_drop_supported {
                            "Drop a video, adjust the appearance,\nthen preview its first frame."
                        } else {
                            "Choose a video, adjust the appearance,\nthen preview its first frame."
                        })
                        .size(18.0)
                        .weak(),
                    );
                    });
                }
            });
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if let Some(job) = &self.job {
            job.cancellation.cancel();
        }
    }
}

/// Reveal the committed file, falling back to its directory on Linux desktops
/// without the FileManager1 selection interface. Run only on the location worker.
fn reveal_output(path: &std::path::Path) -> anyhow::Result<()> {
    use anyhow::{Context, ensure};
    use std::process::Command;
    let path = std::path::absolute(path).context("locate the completed output")?;
    ensure!(
        path.is_file(),
        "the completed output is no longer a regular file"
    );
    #[cfg(target_os = "linux")]
    {
        let uri = output_file_uri(&path);
        let selection = Command::new("gdbus")
            .args([
                "call",
                "--session",
                "--timeout",
                "5",
                "--dest",
                "org.freedesktop.FileManager1",
                "--object-path",
                "/org/freedesktop/FileManager1",
                "--method",
                "org.freedesktop.FileManager1.ShowItems",
            ])
            .arg(format!("['{uri}']"))
            .arg("")
            .output();
        if selection
            .as_ref()
            .is_ok_and(|output| output.status.success())
        {
            return Ok(());
        }
        let parent = path.parent().context("output has no parent directory")?;
        let opened = Command::new("xdg-open")
            .arg(parent)
            .output()
            .context("start the desktop file manager (xdg-open)")?;
        ensure!(
            opened.status.success(),
            "file manager exited with {}: {}",
            opened.status,
            String::from_utf8_lossy(&opened.stderr).trim()
        );
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // Keep the switch separate from the quoted, literal path; never invoke a shell.
        Command::new("explorer.exe")
            .raw_arg("/select,")
            .arg(&path)
            .spawn()
            .context("start Windows Explorer")?;
        Ok(())
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let opened = Command::new("open")
            .arg("-R")
            .arg(&path)
            .output()
            .context("start the file manager")?;
        ensure!(
            opened.status.success(),
            "file manager exited with {}",
            opened.status
        );
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn output_file_uri(path: &std::path::Path) -> String {
    use std::{fmt::Write, os::unix::ffi::OsStrExt};
    let mut uri = String::from("file://");
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            write!(uri, "%{byte:02X}").expect("writing into a string cannot fail");
        }
    }
    uri
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "opens a real desktop file manager; run only during desktop acceptance"]
    fn desktop_reveals_completed_output() {
        let directory =
            std::env::temp_dir().join(format!("asciiflow-location-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let output = directory.join("中文 saved output.mp4");
        std::fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/media/no-audio.mp4"),
            &output,
        )
        .unwrap();
        reveal_output(&output).unwrap();
        assert!(output.is_file());
        println!("Desktop acceptance file: {}", output.display());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn location_uri_preserves_unicode_and_literal_filename_characters() {
        use std::os::unix::ffi::OsStringExt;
        let path = PathBuf::from(OsString::from_vec(
            b"/tmp/\xe4\xb8\xad name ' [x]#?%\xff.mp4".to_vec(),
        ));
        assert_eq!(
            output_file_uri(&path),
            "file:///tmp/%E4%B8%AD%20name%20%27%20%5Bx%5D%23%3F%25%FF.mp4"
        );
    }

    #[test]
    fn dropping_a_new_input_clears_the_previous_destination_and_preview() {
        let ctx = egui::Context::default();
        let mut app = App::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.settings.output = Some("previous.mp4".into());
        app.preview_settings = Some(app.settings.clone());
        app.preview_description = "previous preview".into();
        let path = PathBuf::from("中文 video.mp4");
        let input = egui::RawInput {
            dropped_files: vec![egui::DroppedFile {
                path: Some(path.clone()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut frame = eframe::Frame::_new_kittest();
        let _ = ctx.run(input, |ctx| eframe::App::update(&mut app, ctx, &mut frame));
        assert_eq!(app.settings.input, Some(path));
        assert!(app.settings.output.is_none());
        assert!(app.preview_settings.is_none());
        assert!(app.preview_description.is_empty());
        assert!(!app.failed);
    }

    #[cfg(not(feature = "lgpl-prebuilt"))]
    #[test]
    fn worker_can_convert_repeatedly_after_a_cancellation_request() {
        use asciiflow_core::FrameSource;
        let directory = std::env::temp_dir().join(format!(
            "asciiflow-gui-repeat-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let input = directory.join("中文 空格输入.mp4");
        std::fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/media/no-audio.mp4"),
            &input,
        )
        .unwrap();
        let ctx = egui::Context::default();
        let mut app = App::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.input(input);
        app.settings.backend = "cpu";
        app.settings.encoder = "software";
        app.settings.audio = "none";
        app.settings.config.grid_width = 16;
        let frame = eframe::Frame::_new_kittest();
        // Cancellation may race with a completed commit; both outcomes must leave
        // the GUI reusable and the filesystem consistent.
        let mut committed = 0;
        let mut last_output = None;
        for (index, cancel) in [false, true, false].into_iter().enumerate() {
            let output = directory.join(format!("中文 输出 {index}.mp4"));
            app.settings.output = Some(output.clone());
            app.start(&ctx, true, &frame);
            let job = app.job.as_ref().expect("worker must start");
            if cancel {
                job.cancellation.cancel();
            }
            let deadline = Instant::now() + std::time::Duration::from_secs(30);
            while app.job.is_some() {
                app.complete(&ctx);
                assert!(Instant::now() < deadline, "worker did not finish");
                thread::sleep(std::time::Duration::from_millis(1));
            }
            if cancel && app.failed {
                assert!(app.status.starts_with("Cancelled."), "{}", app.status);
                assert!(!output.exists());
            } else {
                assert!(!app.failed, "{}", app.status);
                assert!(app.status.starts_with("Conversion complete"));
                let mut decoder = asciiflow_media::Decoder::open(&output).unwrap();
                let mut frames = 0;
                while decoder.next_frame().unwrap().is_some() {
                    frames += 1;
                }
                assert_eq!(frames, 90);
                committed += 1;
                last_output = Some(output.clone());
            }
            assert_eq!(app.completed_output, last_output);
            app.settings.output = Some(directory.join("another destination.mp4"));
            assert_eq!(app.completed_output, last_output);
        }
        assert_eq!(
            std::fs::read_dir(&directory).unwrap().count(),
            1 + committed,
            "only the input and successfully committed outputs should remain"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    fn appearance_frame(
        ctx: &egui::Context,
        app: &mut App,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let frame = eframe::Frame::_new_kittest();
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(390.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.settings(ui, &frame));
            },
        )
    }

    fn click_appearance_text(ctx: &egui::Context, app: &mut App, text: &str) {
        // Settle layout before choosing a text position, including newly opened popups.
        appearance_frame(ctx, app, Vec::new());
        let output = appearance_frame(ctx, app, Vec::new());
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(shape) if shape.galley.job.text == text => {
                    Some(shape.pos + egui::vec2(5.0, 5.0))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing text: {text}"));
        appearance_frame(
            ctx,
            app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }

    #[test]
    fn font_search_stays_open_and_selection_updates_the_rendering_font() {
        let ctx = egui::Context::default();
        let mut app = App::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.font_scan = None;
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/fonts/Inconsolata-Regular.ttf");
        app.fonts = vec![asciiflow_font::SystemFont {
            name: "Inconsolata Regular".into(),
            path: path.clone(),
            face_index: 0,
        }];
        click_appearance_text(&ctx, &mut app, "Built-in · 8 × 8");
        assert!(ctx.memory(|memory| memory.any_popup_open()));
        click_appearance_text(&ctx, &mut app, "Search installed fonts…");
        assert!(
            ctx.memory(|memory| memory.any_popup_open()),
            "search must keep the popup open"
        );
        appearance_frame(
            &ctx,
            &mut app,
            vec![egui::Event::Text("inconsolata".into())],
        );
        click_appearance_text(&ctx, &mut app, "Inconsolata Regular");
        assert!(!ctx.memory(|memory| memory.any_popup_open()));
        assert_eq!(app.font_filter, "inconsolata");
        assert_eq!(app.settings.config.font, path.to_str().unwrap());
        assert_eq!(app.settings.font_face_index, Some(0));
    }

    #[test]
    fn charset_presets_match_cli_and_keep_custom_characters_when_switching() {
        let ctx = egui::Context::default();
        let mut app = App::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.settings.input = Some("input.mp4".into());
        app.settings.output = Some("output.mp4".into());
        assert_eq!(
            app.settings.args().unwrap().resolved_charset(),
            asciiflow_cli::args::resolve_charset("standard")
        );
        click_appearance_text(&ctx, &mut app, "Standard");
        click_appearance_text(&ctx, &mut app, "Detailed");
        assert_eq!(app.settings.config.charset, "detailed");
        assert_eq!(
            app.settings.args().unwrap().resolved_charset(),
            asciiflow_cli::args::resolve_charset("detailed")
        );
        click_appearance_text(&ctx, &mut app, "Detailed");
        click_appearance_text(&ctx, &mut app, "Custom");
        app.settings.config.charset = " .:+=#@".into();
        click_appearance_text(&ctx, &mut app, "Custom");
        click_appearance_text(&ctx, &mut app, "Standard");
        assert_eq!(app.settings.config.charset, "standard");
        click_appearance_text(&ctx, &mut app, "Standard");
        click_appearance_text(&ctx, &mut app, "Custom");
        assert_eq!(app.settings.config.charset, " .:+=#@");
        assert_eq!(app.settings.args().unwrap().resolved_charset(), " .:+=#@");
    }

    #[test]
    fn gui_arguments_preserve_literal_characters_and_paths() {
        let mut settings = Settings {
            input: Some(PathBuf::from("-input with spaces.mp4")),
            output: Some(PathBuf::from("-output with spaces.mp4")),
            ..Default::default()
        };
        settings.config.charset = "- .#@".into();
        settings.config.font = "-font name.ttf".into();
        settings.font_face_index = Some(3);
        let args = settings.args().unwrap();
        assert_eq!(args.input, settings.input.unwrap());
        assert_eq!(args.output, settings.output);
        assert_eq!(args.charset, settings.config.charset);
        assert_eq!(args.font, settings.config.font);
        assert_eq!(args.font_face_index, Some(3));
        assert!(args.no_progress);
    }

    #[cfg(unix)]
    #[test]
    fn gui_arguments_preserve_non_unicode_media_paths() {
        use std::os::unix::ffi::OsStringExt;
        let input = PathBuf::from(OsString::from_vec(b"input-\xff.mp4".to_vec()));
        let settings = Settings {
            input: Some(input.clone()),
            output: Some(PathBuf::from("out.mp4")),
            ..Default::default()
        };
        assert_eq!(settings.args().unwrap().input, input);
    }

    #[test]
    fn close_cancels_without_waiting_and_worker_disconnect_is_reported() {
        let (release, wait) = mpsc::sync_channel::<()>(1);
        let (sender, result) = mpsc::sync_channel(1);
        let handle = thread::spawn(move || {
            let _ = wait.recv();
            drop(sender);
        });
        let settings = Settings::default();
        let cancellation = CancellationToken::new();
        let job = Job {
            handle,
            result,
            cancellation: cancellation.clone(),
            progress: Arc::new(Mutex::new(Progress::default())),
            started: Instant::now(),
            conversion: true,
            settings: settings.clone(),
        };
        let ctx = egui::Context::default();
        let mut app = App::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.job = Some(job);
        app.complete(&ctx);
        assert!(app.job.is_some(), "polling must not join an active worker");
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1120.0, 760.0),
            )),
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let mut frame = eframe::Frame::_new_kittest();
        let output = ctx.run(input, |ctx| eframe::App::update(&mut app, ctx, &mut frame));
        assert!(cancellation.is_cancelled());
        assert!(app.closing);
        assert!(
            app.job.is_some(),
            "closing must defer until the worker exits"
        );
        assert!(
            output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::CancelClose)
        );
        release.send(()).unwrap();
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        while !app.job.as_ref().unwrap().handle.is_finished() {
            assert!(Instant::now() < deadline, "worker did not finish");
            thread::yield_now();
        }
        app.complete(&ctx);
        assert!(app.job.is_none());
        assert!(app.failed);
        assert!(app.status.contains("returned no result"));
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            eframe::App::update(&mut app, ctx, &mut frame)
        });
        assert!(
            output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::Close)
        );
    }
}
