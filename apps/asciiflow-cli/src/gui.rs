//! Native, single-task interface. All media and GPU ownership stays on the worker.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use asciiflow_cli::{Preview, args::Args, preview, run_with_progress};
use asciiflow_core::{AsciiConfig, CancellationToken};
use clap::Parser;
use eframe::egui;
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
            .with_min_inner_size([820.0, 600.0]),
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

#[derive(Clone, PartialEq)]
struct Settings {
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    config: AsciiConfig,
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
            config: AsciiConfig::default(),
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
    file_drop_supported: bool,
    settings: Settings,
    job: Option<Job>,
    texture: Option<egui::TextureHandle>,
    preview_settings: Option<Settings>,
    preview_description: String,
    status: String,
    failed: bool,
    closing: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        #[cfg(target_os = "linux")]
        if let Err(error) = install_system_cjk_font(&cc.egui_ctx) {
            eprintln!("System CJK interface font unavailable: {error:#}");
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
        Self {
            file_drop_supported,
            settings: Settings::default(),
            job: None,
            texture: None,
            preview_settings: None,
            preview_description: String::new(),
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
                        preview(&input, &config, worker_cancellation).map(Outcome::Preview)
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
                self.status = format!(
                    "Conversion complete in {:.1}s. Saved to {}",
                    job.started.elapsed().as_secs_f64(),
                    job.settings
                        .output
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_default()
                );
            }
            Err(error) => self.error(error),
        }
    }

    fn files(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        ui.heading("1  Files");
        if ui.button("Choose input video…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .set_parent(frame)
                .add_filter("MP4 video", &["mp4", "mov", "m4v"])
                .pick_file()
        {
            self.input(path);
        }
        ui.label(
            self.settings
                .input
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| {
                    if self.file_drop_supported {
                    "Or drop a local video into this window."
                } else {
                    "Use Choose input video. File dragging is unavailable in this Wayland window."
                }.into()
                }),
        );
        ui.add_space(8.0);
        if ui.button("Save output as…").clicked() {
            let name = self
                .settings
                .input
                .as_ref()
                .and_then(|path| path.file_stem())
                .map(|stem| format!("{}-ascii.mp4", stem.to_string_lossy()))
                .unwrap_or_else(|| "output-ascii.mp4".into());
            if let Some(path) = rfd::FileDialog::new()
                .set_parent(frame)
                .add_filter("MP4 video", &["mp4"])
                .set_file_name(&name)
                .save_file()
            {
                self.settings.output = Some(path);
            }
        }
        ui.label(
            self.settings
                .output
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "Choose an MP4 destination before converting.".into()),
        );
        ui.small("Replacing an existing file requires confirmation; it is replaced only after conversion succeeds.");
    }

    fn settings(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        ui.heading("2  Appearance");
        egui::Grid::new("appearance").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
            let label = ui.label("Columns");
            ui.add(egui::DragValue::new(&mut self.settings.config.grid_width).range(1..=8192)).labelled_by(label.id); ui.end_row();
            let label = ui.label("Rows");
            let mut height = self.settings.config.grid_height.unwrap_or(0);
            if ui.add(egui::DragValue::new(&mut height).range(0..=8192).custom_formatter(|value, _| if value == 0.0 { "Auto".into() } else { format!("{value:.0}") })).labelled_by(label.id).changed() {
                self.settings.config.grid_height = (height != 0).then_some(height);
            } ui.end_row();
            ui.label("Color"); ui.checkbox(&mut self.settings.config.color, "Retain source colors"); ui.end_row();
            let label = ui.label("Character set"); ui.text_edit_singleline(&mut self.settings.config.charset).labelled_by(label.id); ui.end_row();
            let label = ui.label("Font"); ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.settings.config.font).desired_width(155.0)).labelled_by(label.id);
                if ui.small_button("Browse…").clicked()
                    && let Some(path) = rfd::FileDialog::new().set_parent(frame).add_filter("Font", &["ttf", "otf", "ttc"]).pick_file()
                {
                    match path.to_str() {
                        Some(path) => self.settings.config.font = path.into(),
                        None => self.error("This font path cannot be represented as Unicode. Choose another path.".into()),
                    }
                }
            }); ui.end_row();
            let label = ui.label("Audio"); choice(ui, "audio", &mut self.settings.audio, &[("auto", "Auto"), ("copy", "Require passthrough"), ("none", "No audio")]).labelled_by(label.id); ui.end_row();
        });
        ui.small("Rows: 0 = automatic. Characters run from sparse to dense. Font: builtin-8x8 or a monospaced font file.");
        ui.add_space(8.0);
        ui.collapsing("Advanced processing", |ui| {
            egui::Grid::new("advanced").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
                let label = ui.label("Processing"); choice(ui, "backend", &mut self.settings.backend, &[("auto", "Auto"), ("cpu", "CPU"), ("vulkan", "Vulkan")]).labelled_by(label.id); ui.end_row();
                let label = ui.label("Encoder");
                choice(ui, "encoder", &mut self.settings.encoder, &[("auto", "Auto"), ("vaapi", "VAAPI (Linux)"), ("mf", "Media Foundation (Windows)"), ("software", "Software (custom build)")]).labelled_by(label.id); ui.end_row();
                let label = ui.label("Video codec"); choice(ui, "codec", &mut self.settings.codec, &[("h264", "H.264"), ("hevc", "HEVC"), ("av1", "AV1")]).labelled_by(label.id); ui.end_row();
                let label = ui.label("Bit depth"); choice(ui, "depth", &mut self.settings.depth, &[("8", "8 bit"), ("10", "10 bit")]).labelled_by(label.id); ui.end_row();
                let label = ui.label("Dynamic range"); choice(ui, "range", &mut self.settings.dynamic_range, &[("preserve", "Preserve"), ("sdr", "Convert to SDR")]).labelled_by(label.id); ui.end_row();
            });
            ui.small("Hardware support is checked when converting. Official LGPL packages require a supported hardware encoder. HEVC, AV1 and HDR require qualified Linux hardware paths.");
        });
    }
}

/// Use an installed font for file names; never bundle or change the video font.
#[cfg(target_os = "linux")]
fn install_system_cjk_font(ctx: &egui::Context) -> anyhow::Result<()> {
    let resolved = asciiflow_font::resolve_font("Noto Sans CJK SC:charset=4e2d 6587", None)?;
    let path = resolved
        .path
        .ok_or_else(|| anyhow::anyhow!("Fontconfig returned no font file"))?;
    let mut data = egui::FontData::from_owned(std::fs::read(&path)?);
    // FreeType encodes a named variable-font instance in the high 16 bits.
    // egui accepts only the collection face, using that face's default axes.
    data.index = u32::try_from(resolved.face_index)? & 0xffff;
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("system-cjk".into(), Arc::new(data));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("system-cjk".into());
    }
    ctx.set_fonts(fonts);
    Ok(())
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
        egui::TopBottomPanel::top("title").show(ctx, |ui| {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.heading("AsciiFlow");
                ui.label(egui::RichText::new("VIDEO / ASCII").weak());
            });
            ui.add_space(8.0);
        });
        egui::TopBottomPanel::bottom("task").show(ctx, |ui| {
            ui.add_space(8.0);
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
                                (frames as f64 / total as f64).clamp(0.0, 1.0) as f32
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
                ui.horizontal(|ui| {
                    let ready = self.settings.input.is_some() && !self.closing;
                    if ui
                        .add_enabled(ready, egui::Button::new("Preview first frame"))
                        .clicked()
                    {
                        self.start(ctx, false, frame);
                    }
                    if ui
                        .add_enabled(
                            ready && self.settings.output.is_some(),
                            egui::Button::new("Start conversion")
                                .fill(egui::Color32::from_rgb(34, 104, 130)),
                        )
                        .clicked()
                    {
                        self.start(ctx, true, frame);
                    }
                });
            }
            ui.add_space(4.0);
            ui.label(egui::RichText::new(&self.status).color(if self.failed {
                egui::Color32::from_rgb(255, 155, 145)
            } else {
                ui.visuals().text_color()
            }));
            ui.add_space(8.0);
        });
        egui::SidePanel::left("controls")
            .resizable(false)
            .exact_width(370.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_enabled_ui(self.job.is_none() && !self.closing, |ui| {
                        ui.add_space(12.0);
                        self.files(ui, frame);
                        ui.add_space(18.0);
                        self.settings(ui, frame);
                    });
                });
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("First-frame preview");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires a Linux system CJK font; desktop acceptance gate"]
    fn desktop_system_font_displays_chinese_file_names() {
        let ctx = egui::Context::default();
        install_system_cjk_font(&ctx).unwrap();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            ctx.fonts(|fonts| {
                for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                    let font = egui::FontId::new(14.0, family);
                    for character in "中文測試视频.mp4".chars() {
                        assert!(fonts.has_glyph(&font, character), "missing {character}");
                    }
                }
            });
        });
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
            }
        }
        assert_eq!(
            std::fs::read_dir(&directory).unwrap().count(),
            1 + committed,
            "only the input and successfully committed outputs should remain"
        );
        std::fs::remove_dir_all(directory).unwrap();
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
        let args = settings.args().unwrap();
        assert_eq!(args.input, settings.input.unwrap());
        assert_eq!(args.output, settings.output);
        assert_eq!(args.charset, settings.config.charset);
        assert_eq!(args.font, settings.config.font);
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
        let mut app = App {
            file_drop_supported: true,
            settings,
            job: Some(job),
            texture: None,
            preview_settings: None,
            preview_description: String::new(),
            status: String::new(),
            failed: false,
            closing: false,
        };
        let ctx = egui::Context::default();
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
