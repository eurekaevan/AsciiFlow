mod args;
mod capabilities;
mod display;

use anyhow::{Context, Result, bail};
use args::{Args, VulkanMappingArg};
use asciiflow_core::{
    AsciiBackend, AsciiConfig, AudioPlan, CancellationToken, CapabilitySnapshot, CapabilitySupport,
    FrameSource, MediaImplementation, Pipeline, PipelinePlan, PipelinePlanner, PipelinePolicy,
    PipelineStage, PlanningResult, ProcessingBackend, SourceTimings,
};
use asciiflow_cpu::{CpuAsciiBackend, Nv12Mapper};
use asciiflow_interop::{
    VaapiVulkanFullInteropProcessor, VaapiVulkanInteropProcessor,
    VulkanVaapiOutputInteropProcessor, run_full_interop_pipeline_with_cancellation,
    run_interop_pipeline_with_cancellation, run_output_interop_pipeline_with_cancellation,
};
use asciiflow_media::{
    DecodeMode, Decoder, EncodeMode, Encoder, MediaInfo, OutputEncoding, VaapiOptions,
};
use asciiflow_vulkan::{DeviceInfo, PipelinedVulkanAsciiBackend, VulkanAsciiBackend};
use clap::Parser;
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    let args = Args::parse();
    let filter = if args.verbose {
        "asciiflow=debug"
    } else {
        "asciiflow=info"
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter)),
        )
        .with_target(false)
        .init();
    let cancellation = CancellationToken::new();
    let signal_token = cancellation.clone();
    if let Err(error) = ctrlc::set_handler(move || signal_token.cancel()) {
        eprintln!("Error: cancellation initialization failed: install SIGINT handler: {error}");
        return ExitCode::FAILURE;
    }
    match run(args, cancellation) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if is_cancellation(&error) {
                eprintln!("Cancelled: conversion stopped before output commit");
                ExitCode::from(failure_exit_code(&error))
            } else {
                eprintln!("Error: {error}");
                ExitCode::FAILURE
            }
        }
    }
}

fn is_cancellation(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<asciiflow_core::Error>()
            .is_some_and(asciiflow_core::Error::is_cancelled)
    })
}

fn failure_exit_code(error: &anyhow::Error) -> u8 {
    if is_cancellation(error) { 130 } else { 1 }
}

fn run(args: Args, cancellation: CancellationToken) -> Result<()> {
    if args.output.is_none() && !args.capabilities && !args.explain_plan {
        bail!("output path is required unless --capabilities or --explain-plan is used");
    }
    if let Some(output) = &args.output {
        if output
            .extension()
            .and_then(|value| value.to_str())
            .is_none_or(|value| !value.eq_ignore_ascii_case("mp4"))
        {
            bail!("the current output container is MP4; use an .mp4 path");
        }
        if paths_refer_to_same_file(&args.input, output) {
            bail!("input and output must be different paths");
        }
    }
    if args.backend == args::BackendArg::Cpu && args.vulkan_mapping != VulkanMappingArg::Auto {
        bail!("--vulkan-mapping requires --backend auto or --backend vulkan");
    }
    if args.vulkan_mapping == VulkanMappingArg::Cpu
        && (args.vaapi_vulkan_input_interop == args::InteropArg::On
            || args.vaapi_vulkan_output_interop == args::InteropArg::On)
    {
        bail!("hardware interop cannot be combined with diagnostic CPU Vulkan mapping");
    }
    let config = AsciiConfig {
        grid_width: args.width,
        grid_height: args.height,
        charset: args.resolved_charset(),
        font: args.font.clone(),
        color: args.color,
    };
    config.validate()?;
    let policy = PipelinePolicy {
        backend: args.backend.into(),
        decode: args.decode.into(),
        encode: args.encode.into(),
        input_interop: if args.vulkan_mapping == VulkanMappingArg::Cpu {
            asciiflow_core::InteropRequest::Off
        } else {
            args.vaapi_vulkan_input_interop.into()
        },
        output_interop: if args.vulkan_mapping == VulkanMappingArg::Cpu {
            asciiflow_core::InteropRequest::Off
        } else {
            args.vaapi_vulkan_output_interop.into()
        },
        output_codec: args.output_codec.into(),
        output_bit_depth: args.output_bit_depth.into(),
    };
    PipelinePlanner::validate_policy(policy.clone()).map_err(|error| {
        asciiflow_core::Error::pipeline(PipelineStage::Planning, "validate pipeline policy", error)
    })?;
    if args.explain_plan {
        // Diagnostics may need to explain an input rejected by first-frame
        // color qualification, before the full capability probe can return.
        let mut inspection = Decoder::open(&args.input).map_err(|error| {
            asciiflow_core::Error::pipeline(PipelineStage::InputProbe, "open input media", error)
        })?;
        if let Err(error) = inspection.next_frame() {
            capabilities::print_rejected_color(&inspection.info().requirements);
            return Err(asciiflow_core::Error::pipeline(
                PipelineStage::InputProbe,
                "decode input qualification frame",
                error,
            )
            .into());
        }
    }
    let vaapi = VaapiOptions::new(args.hw_device.clone());
    let probe = capabilities::probe(&args.input, &vaapi, &config)?;
    ensure_not_cancelled(&cancellation)?;
    let planning_started = Instant::now();
    let mut snapshot = probe.snapshot;
    let decision =
        PipelinePlanner::select(&snapshot, &probe.media_info.requirements, policy.clone())
            .map_err(|error| {
                asciiflow_core::Error::pipeline(
                    PipelineStage::Planning,
                    "select initial pipeline",
                    error,
                )
            })?;
    let planning_duration = planning_started.elapsed();
    let audio_plan = AudioPlan::select(args.audio.into(), &probe.media_info.audio_streams)
        .map_err(|error| {
            asciiflow_core::Error::pipeline(
                PipelineStage::Planning,
                "select audio passthrough streams",
                error,
            )
        })?;
    if args.capabilities {
        capabilities::print(&snapshot, &probe.media_info, &audio_plan, probe.duration);
        if args.explain_plan {
            println!();
            print_plan_explanation(&decision, &audio_plan, planning_duration);
        }
        return Ok(());
    }
    if args.explain_plan {
        capabilities::print(&snapshot, &probe.media_info, &audio_plan, probe.duration);
        println!();
        print_plan_explanation(&decision, &audio_plan, planning_duration);
        return Ok(());
    }
    let output = args
        .output
        .as_deref()
        .context("output path is required unless --capabilities is used")?;
    let info = probe.media_info;
    ensure_not_cancelled(&cancellation)?;
    let atlas = if args.font == "builtin-8x8" {
        if args.font_face_index != 0 {
            bail!("--font-face-index requires a font file");
        }
        asciiflow_font::GlyphAtlas::builtin(&args.font, &config.charset)?
    } else {
        let (columns, rows) =
            config.resolved_grid(info.frame_desc.width, info.frame_desc.height)?;
        let (atlas, diagnostics) = asciiflow_font::build_font_atlas(
            Path::new(&args.font),
            args.font_face_index as isize,
            &config.charset,
            info.frame_desc.width.div_ceil(columns),
            info.frame_desc.height.div_ceil(rows),
        )?;
        if args.verbose {
            println!(
                "Font: FreeType {:?} · {} {} · face {} · {} glyphs · {}x{} R8 tiles · {} bytes · ppem {} · baseline {} · load {:.3} ms · build {:.3} ms",
                diagnostics.version,
                diagnostics.family,
                diagnostics.style,
                args.font_face_index,
                atlas.glyph_count(),
                atlas.width(),
                atlas.height(),
                atlas.as_r8_slice().len(),
                diagnostics.pixel_size,
                diagnostics.baseline,
                diagnostics.load_wall.as_secs_f64() * 1000.0,
                diagnostics.build_wall.as_secs_f64() * 1000.0
            );
        }
        atlas
    };
    let temporary = temporary_output_path(output)?;
    prepare_temporary_output(&temporary)?;
    let mut temporary_guard = TemporaryOutputGuard::new(temporary.clone());
    let initialized = initialize_with_replan(
        &mut snapshot,
        &info.requirements,
        policy,
        decision,
        |plan| {
            PipelineFactory::build(
                plan,
                FactoryContext {
                    atlas: &atlas,
                    audio_plan: &audio_plan,
                    args: &args,
                    info: &info,
                    config: &config,
                    vaapi: &vaapi,
                    temporary: &temporary,
                    cancellation: &cancellation,
                },
            )
        },
        || {
            let _ = fs::remove_file(&temporary);
            Ok(())
        },
    )?;
    let built = initialized.value;
    let decision = initialized.decision;
    let replan = initialized.replan;
    ensure_not_cancelled(&cancellation)?;
    let plan = decision.selected;
    if args.verbose {
        capabilities::print_plan(&plan);
        capabilities::print_audio_plan(&audio_plan);
        println!(
            "Startup: capability probe {:.3} ms · planning {:.3} ms · fallback count {}",
            probe.duration.as_secs_f64() * 1e3,
            planning_duration.as_secs_f64() * 1e3,
            usize::from(replan.is_some()),
        );
        if let Some(replan) = &replan {
            println!(
                "Initial plan:\n  {}\nInitialization failed:\n  {}\nReplanned:\n  {}\nReplanning CPU wall: {:.3} ms",
                replan.initial_plan,
                replan.first_failure,
                replan.replanned_plan,
                replan.duration.as_secs_f64() * 1e3
            );
        }
    }
    if args.audio == args::AudioArg::Auto {
        for skipped in &audio_plan.skipped {
            eprintln!(
                "Warning: audio stream #{} ({}) was skipped: {}",
                skipped.input_index, skipped.codec, skipped.reason
            );
        }
    }
    let BuiltExecution {
        decoder,
        encoder,
        selection,
    } = built;
    let BackendSelection {
        processor,
        device_info,
        memory_allocations,
        mapping_strategy,
        gpu_slots,
    } = selection;
    let host_format = match info.frame_desc.format {
        asciiflow_core::PixelFormat::Nv12 => "Host NV12",
        asciiflow_core::PixelFormat::P010Le => "Host P010LE",
    };
    println!(
        "输入 {}x{} · {:.3} FPS · {} · {:?} backend · {} 槽",
        info.frame_desc.width,
        info.frame_desc.height,
        info.frame_rate.numerator as f64 / info.frame_rate.denominator as f64,
        if plan.hardware_input_interop {
            "VAAPI DRM PRIME input"
        } else {
            host_format
        },
        plan.backend,
        plan.buffer_capacity
    );
    let mut media_plan = vec![match plan.decode {
        MediaImplementation::Software => "software decode",
        MediaImplementation::Hardware => "VAAPI decode",
    }];
    if plan.hardware_input_interop {
        media_plan.extend([
            "DRM PRIME direct map",
            "DMA-BUF external image",
            "GPU image→buffer copy",
            "Vulkan ASCII",
        ]);
    } else {
        if plan.hardware_download {
            media_plan.push("hwdownload");
        }
        media_plan.push(host_format);
        media_plan.push(match plan.backend {
            ProcessingBackend::Cpu => "CPU ASCII",
            ProcessingBackend::Vulkan => "Vulkan ASCII",
            ProcessingBackend::Auto => unreachable!("planner must resolve Auto"),
        });
    }
    if plan.hardware_output_interop {
        media_plan.extend([
            "DRM PRIME WRITE direct map",
            "DMA-BUF writable external image",
            "GPU buffer→image copy",
            "VAAPI encode",
        ]);
    } else {
        media_plan.push(host_format);
        if plan.hardware_upload {
            media_plan.push("hwupload");
        }
        media_plan.push(match plan.encode {
            MediaImplementation::Software => "software encode",
            MediaImplementation::Hardware => "VAAPI encode",
        });
    }
    println!("Media plan：{}", media_plan.join(" → "));
    if audio_plan.selected.is_empty() {
        println!("Audio：none");
    } else {
        println!(
            "Audio：{} compressed stream(s) → packet passthrough → MP4 mux",
            audio_plan.selected.len()
        );
    }
    if plan.hardware_download
        || plan.hardware_upload
        || plan.hardware_input_interop
        || plan.hardware_output_interop
    {
        println!("Media HW backend：VAAPI");
        println!("Device：{}", vaapi.display_device());
    }
    if let Some(info) = device_info {
        println!("Vulkan 设备：{}", info.name);
        println!(
            "Vulkan mapping：{}{}",
            mapping_strategy.expect("Vulkan mapping strategy missing"),
            if args.vulkan_mapping == VulkanMappingArg::Auto {
                " (auto)"
            } else {
                ""
            }
        );
        println!(
            "Vulkan frame slots：{}",
            gpu_slots.expect("Vulkan slot count missing")
        );
        if args.verbose {
            println!(
                "GPU vendor/device {:#06x}/{:#06x} · {:?} · Vulkan {} · driver {} · compute queue {}",
                info.vendor_id,
                info.device_id,
                info.device_type,
                info.api_version_string(),
                info.driver_version,
                info.queue_family,
            );
            for heap in &info.memory_heaps {
                println!(
                    "Vulkan memory heap {}: {} bytes · {:?}",
                    heap.index, heap.size, heap.flags
                );
            }
            for memory_type in &info.memory_types {
                println!(
                    "Vulkan memory type {}: heap {} · {:?}",
                    memory_type.index, memory_type.heap_index, memory_type.property_flags
                );
            }
            for allocation in &memory_allocations {
                println!(
                    "Vulkan allocation {}: type {} · heap {} · {:?}",
                    allocation.name,
                    allocation.memory_type_index,
                    allocation.heap_index,
                    allocation.property_flags
                );
            }
        }
    }
    if let Some(frames) = info.frame_count {
        println!("源视频帧数：{frames}");
    }
    let max_frames = (args.max_frames != 0).then_some(args.max_frames);
    let result = match processor {
        SelectedProcessor::Host(backend) => {
            let source = LimitedSource {
                inner: decoder,
                remaining: max_frames,
            };
            Pipeline::new(plan.buffer_capacity)?
                .run_with_cancellation(source, backend, encoder, config, cancellation.clone())
                .map(|report| report.metrics)
        }
        SelectedProcessor::Interop(interop) => run_interop_pipeline_with_cancellation(
            decoder,
            *interop,
            encoder,
            max_frames,
            plan.buffer_capacity,
            cancellation.clone(),
        ),
        SelectedProcessor::FullInterop(interop) => run_full_interop_pipeline_with_cancellation(
            decoder,
            *interop,
            encoder,
            max_frames,
            plan.buffer_capacity,
            cancellation.clone(),
        ),
        SelectedProcessor::OutputInterop(interop) => run_output_interop_pipeline_with_cancellation(
            decoder,
            *interop,
            encoder,
            max_frames,
            plan.buffer_capacity,
            cancellation.clone(),
        ),
    };
    match result {
        Ok(metrics) => {
            commit_output(&temporary, output)?;
            temporary_guard.disarm();
            display::print_summary(&metrics);
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error.into())
        }
    }
}

fn ensure_not_cancelled(cancellation: &CancellationToken) -> Result<()> {
    if cancellation.is_cancelled() {
        Err(asciiflow_core::Error::Cancelled.into())
    } else {
        Ok(())
    }
}

fn print_plan_explanation(
    decision: &PlanningResult,
    audio_plan: &AudioPlan,
    planning_duration: Duration,
) {
    println!("Rejected candidates:");
    for rejection in &decision.rejected {
        println!("  - {}", rejection.candidate);
        for reason in &rejection.reasons {
            println!("      {reason}");
        }
    }
    if decision.rejected.is_empty() {
        println!("  (none)");
    }
    println!();
    capabilities::print_plan(&decision.selected);
    capabilities::print_audio_plan(audio_plan);
    println!(
        "Planning CPU wall: {:.3} ms",
        planning_duration.as_secs_f64() * 1e3
    );
}

struct HybridAsciiBackend {
    mapper_key: Option<String>,
    mapper: Option<Nv12Mapper>,
    cells: Vec<asciiflow_vulkan::GpuAsciiCell>,
    vulkan: VulkanAsciiBackend,
}

struct BackendSelection {
    processor: SelectedProcessor,
    device_info: Option<DeviceInfo>,
    memory_allocations: Vec<asciiflow_vulkan::MemoryAllocationInfo>,
    mapping_strategy: Option<&'static str>,
    gpu_slots: Option<usize>,
}

struct BuiltExecution {
    decoder: Decoder,
    encoder: Encoder,
    selection: BackendSelection,
}

#[derive(Clone, Copy, Debug)]
enum InitCapability {
    SoftwareDecode,
    HardwareDecode,
    Vulkan,
    SoftwareEncode,
    HardwareEncode,
    InputInterop,
    OutputInterop,
    Muxer,
    Configuration,
}

impl InitCapability {
    fn stage(self) -> PipelineStage {
        match self {
            Self::SoftwareDecode | Self::HardwareDecode => PipelineStage::DecoderInitialization,
            Self::Vulkan => PipelineStage::ProcessorInitialization,
            Self::SoftwareEncode | Self::HardwareEncode => PipelineStage::EncoderInitialization,
            Self::InputInterop => PipelineStage::InputInteropInitialization,
            Self::OutputInterop => PipelineStage::OutputInteropInitialization,
            Self::Muxer => PipelineStage::MuxInitialization,
            Self::Configuration => PipelineStage::Planning,
        }
    }

    fn operation(self) -> &'static str {
        match self {
            Self::SoftwareDecode => "create software decoder",
            Self::HardwareDecode => "create VAAPI input video decoder",
            Self::Vulkan => "create Vulkan ASCII processor",
            Self::SoftwareEncode => "create software output video encoder and MP4 muxer",
            Self::HardwareEncode => "create VAAPI output video encoder and MP4 muxer",
            Self::InputInterop => "create VAAPI to Vulkan input interop",
            Self::OutputInterop => "create Vulkan to VAAPI output interop",
            Self::Muxer => "create MP4 muxer",
            Self::Configuration => "validate execution configuration",
        }
    }

    fn is_auto(self, policy: &PipelinePolicy) -> bool {
        match self {
            Self::HardwareDecode => policy.decode == asciiflow_core::MediaRequest::Auto,
            Self::Vulkan => policy.backend == ProcessingBackend::Auto,
            Self::HardwareEncode => policy.encode == asciiflow_core::MediaRequest::Auto,
            Self::InputInterop => policy.input_interop == asciiflow_core::InteropRequest::Auto,
            Self::OutputInterop => policy.output_interop == asciiflow_core::InteropRequest::Auto,
            Self::SoftwareDecode | Self::SoftwareEncode | Self::Muxer | Self::Configuration => {
                false
            }
        }
    }

    fn mark_unsupported(
        self,
        snapshot: &mut CapabilitySnapshot,
        input: &asciiflow_core::InputRequirements,
        output: &asciiflow_core::OutputVideoRequirements,
        reason: &str,
    ) {
        let unsupported = || CapabilitySupport::unsupported(reason);
        match self {
            Self::HardwareDecode => match (&input.codec, input.bit_depth) {
                (asciiflow_core::VideoCodec::Hevc, Some(10)) => {
                    snapshot.media.hevc_main10_vaapi_decode = unsupported()
                }
                (asciiflow_core::VideoCodec::Av1, Some(10)) => {
                    snapshot.media.av1_10bit_vaapi_decode = unsupported()
                }
                _ => snapshot.media.disable_decode(&input.codec, reason),
            },
            Self::Vulkan => {
                snapshot.processing.vulkan = unsupported();
                snapshot.processing.vulkan_auto_eligible = false;
            }
            Self::HardwareEncode => snapshot.media.disable_encode_output(output, reason),
            Self::InputInterop => {
                if input.bit_depth == Some(10) {
                    snapshot.interop.p010_input = unsupported();
                } else {
                    snapshot.interop.set_input(&input.codec, unsupported());
                }
            }
            Self::OutputInterop => snapshot
                .interop
                .set_output_for_requirements(output, unsupported()),
            Self::SoftwareDecode | Self::SoftwareEncode | Self::Muxer | Self::Configuration => {}
        }
    }
}

#[derive(Debug)]
struct InitializationFailure {
    capability: InitCapability,
    source: anyhow::Error,
}

impl InitializationFailure {
    fn new(capability: InitCapability, source: impl Into<anyhow::Error>) -> Self {
        Self {
            capability,
            source: source.into(),
        }
    }
}

impl std::fmt::Display for InitializationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}: {} failed: {:#}",
            self.capability.stage(),
            self.capability.operation(),
            self.source
        )
    }
}

#[derive(Debug)]
struct ReplanRecord {
    initial_plan: PipelinePlan,
    first_failure: String,
    replanned_plan: PipelinePlan,
    duration: Duration,
}

#[derive(Debug)]
struct Initialized<T> {
    value: T,
    decision: PlanningResult,
    replan: Option<ReplanRecord>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InitializationPoint {
    DecoderCreate,
    VaapiFramesPoolCreate,
    InputDrmPrimeMap,
    InputDmaBufImport,
    VulkanProcessorCreate,
    OutputVaapiFrameAcquire,
    OutputDrmPrimeMap,
    OutputDmaBufImport,
    EncoderCreate,
    MuxerCreate,
}

impl InitializationPoint {
    #[cfg(test)]
    fn capability(self, plan: &PipelinePlan) -> InitCapability {
        match self {
            Self::DecoderCreate => match plan.decode {
                MediaImplementation::Software => InitCapability::SoftwareDecode,
                MediaImplementation::Hardware => InitCapability::HardwareDecode,
            },
            Self::VaapiFramesPoolCreate => InitCapability::HardwareEncode,
            Self::OutputVaapiFrameAcquire | Self::OutputDrmPrimeMap | Self::OutputDmaBufImport => {
                InitCapability::OutputInterop
            }
            Self::InputDrmPrimeMap | Self::InputDmaBufImport => InitCapability::InputInterop,
            Self::VulkanProcessorCreate => InitCapability::Vulkan,
            Self::EncoderCreate => match plan.encode {
                MediaImplementation::Software => InitCapability::SoftwareEncode,
                MediaImplementation::Hardware => InitCapability::HardwareEncode,
            },
            Self::MuxerCreate => InitCapability::Muxer,
        }
    }

    #[cfg(test)]
    fn operation(self) -> &'static str {
        match self {
            Self::DecoderCreate => "decoder create",
            Self::VaapiFramesPoolCreate => "VAAPI frames pool create",
            Self::InputDrmPrimeMap => "input DRM PRIME map",
            Self::InputDmaBufImport => "input DMA-BUF import",
            Self::VulkanProcessorCreate => "Vulkan processor create",
            Self::OutputVaapiFrameAcquire => "output VAAPI frame acquire",
            Self::OutputDrmPrimeMap => "output DRM PRIME map",
            Self::OutputDmaBufImport => "output DMA-BUF import",
            Self::EncoderCreate => "encoder create",
            Self::MuxerCreate => "muxer create",
        }
    }
}

trait FactoryHooks {
    fn checkpoint(
        &mut self,
        _point: InitializationPoint,
        _plan: &PipelinePlan,
    ) -> std::result::Result<(), InitializationFailure> {
        Ok(())
    }
}

struct ProductionFactoryHooks;
impl FactoryHooks for ProductionFactoryHooks {}

#[cfg(test)]
struct InitializationFaultInjector {
    point: InitializationPoint,
    fired: bool,
}

#[cfg(test)]
impl InitializationFaultInjector {
    fn new(point: InitializationPoint) -> Self {
        Self {
            point,
            fired: false,
        }
    }
}

#[cfg(test)]
impl FactoryHooks for InitializationFaultInjector {
    fn checkpoint(
        &mut self,
        point: InitializationPoint,
        plan: &PipelinePlan,
    ) -> std::result::Result<(), InitializationFailure> {
        if !self.fired && point == self.point {
            self.fired = true;
            Err(InitializationFailure::new(
                point.capability(plan),
                anyhow::anyhow!("deterministic injected failure at {}", point.operation()),
            ))
        } else {
            Ok(())
        }
    }
}

fn initialize_with_replan<T>(
    snapshot: &mut CapabilitySnapshot,
    requirements: &asciiflow_core::InputRequirements,
    policy: PipelinePolicy,
    initial: PlanningResult,
    mut build: impl FnMut(&PipelinePlan) -> std::result::Result<T, InitializationFailure>,
    mut reset_staging: impl FnMut() -> Result<()>,
) -> Result<Initialized<T>> {
    let initial_plan = initial.selected.clone();
    match build(&initial_plan) {
        Ok(value) => Ok(Initialized {
            value,
            decision: initial,
            replan: None,
        }),
        Err(first) if first.capability.is_auto(&policy) => {
            let first_failure = first.to_string();
            first.capability.mark_unsupported(
                snapshot,
                requirements,
                &initial_plan.output,
                &first_failure,
            );
            reset_staging().context("initialization replan: clean failed staging output")?;
            let started = Instant::now();
            let replanned = PipelinePlanner::select(snapshot, requirements, policy.clone())
                .map_err(|replan_failure| {
                    anyhow::anyhow!(
                        "pipeline initialization could not find a legal automatic replan\ninitial plan: {}\nfirst failure: {}\nreplan failure: {}",
                        initial_plan,
                        first_failure,
                        replan_failure
                    )
                })?;
            let duration = started.elapsed();
            let replanned_plan = replanned.selected.clone();
            match build(&replanned_plan) {
                Ok(value) => Ok(Initialized {
                    value,
                    decision: replanned,
                    replan: Some(ReplanRecord {
                        initial_plan,
                        first_failure,
                        replanned_plan,
                        duration,
                    }),
                }),
                Err(second) => Err(anyhow::anyhow!(
                    "pipeline initialization failed after one automatic replan\ninitial plan: {}\nfirst failure: {}\nreplanned plan: {}\nsecond failure: {}",
                    initial_plan,
                    first_failure,
                    replanned_plan,
                    second
                )),
            }
        }
        Err(failure) => Err(anyhow::anyhow!(
            "pipeline initialization failed\nselected plan: {}\nfailure: {}",
            initial_plan,
            failure
        )),
    }
}

struct PipelineFactory;

#[derive(Clone, Copy)]
struct FactoryContext<'a> {
    atlas: &'a asciiflow_font::GlyphAtlas,
    audio_plan: &'a AudioPlan,
    args: &'a Args,
    info: &'a MediaInfo,
    config: &'a AsciiConfig,
    vaapi: &'a VaapiOptions,
    temporary: &'a Path,
    cancellation: &'a CancellationToken,
}

impl PipelineFactory {
    fn build(
        plan: &PipelinePlan,
        context: FactoryContext<'_>,
    ) -> std::result::Result<BuiltExecution, InitializationFailure> {
        let mut hooks = ProductionFactoryHooks;
        Self::build_with_hooks(plan, context, &mut hooks)
    }

    fn build_with_hooks(
        plan: &PipelinePlan,
        context: FactoryContext<'_>,
        hooks: &mut impl FactoryHooks,
    ) -> std::result::Result<BuiltExecution, InitializationFailure> {
        let FactoryContext {
            atlas,
            audio_plan,
            args,
            info,
            config,
            vaapi,
            temporary,
            cancellation,
        } = context;
        hooks.checkpoint(InitializationPoint::DecoderCreate, plan)?;
        let decode_capability = match plan.decode {
            MediaImplementation::Software => InitCapability::SoftwareDecode,
            MediaImplementation::Hardware => InitCapability::HardwareDecode,
        };
        let decode_mode = match plan.decode {
            MediaImplementation::Software => DecodeMode::Software,
            MediaImplementation::Hardware => DecodeMode::Vaapi,
        };
        let mut decoder = Decoder::open_with(&args.input, decode_mode, vaapi.clone())
            .map_err(|error| InitializationFailure::new(decode_capability, error))?;
        let audio_templates = decoder
            .audio_output_templates(audio_plan)
            .map_err(|error| InitializationFailure::new(InitCapability::Muxer, error))?;

        let vulkan = if plan.backend == ProcessingBackend::Vulkan {
            hooks.checkpoint(InitializationPoint::VulkanProcessorCreate, plan)?;
            Some(
                VulkanAsciiBackend::new()
                    .and_then(|backend| backend.with_atlas(atlas.clone(), config))
                    .and_then(|mut backend| {
                        backend.prepare(&info.frame_desc, config)?;
                        Ok(backend)
                    })
                    .map_err(|error| InitializationFailure::new(InitCapability::Vulkan, error))?,
            )
        } else {
            None
        };

        let encode_capability = match plan.encode {
            MediaImplementation::Software => InitCapability::SoftwareEncode,
            MediaImplementation::Hardware => InitCapability::HardwareEncode,
        };
        let encode_mode = match plan.encode {
            MediaImplementation::Software => EncodeMode::Software,
            MediaImplementation::Hardware => EncodeMode::Vaapi,
        };
        hooks.checkpoint(InitializationPoint::EncoderCreate, plan)?;
        hooks.checkpoint(InitializationPoint::MuxerCreate, plan)?;
        if plan.encode == MediaImplementation::Hardware {
            hooks.checkpoint(InitializationPoint::VaapiFramesPoolCreate, plan)?;
        }
        let encoder_result = if plan.hardware_output_interop {
            Encoder::create_with_hardware_frames_codec_and_audio(
                temporary,
                info.frame_desc.clone(),
                info.frame_rate,
                plan.output.codec.clone(),
                vaapi.clone(),
                audio_templates,
                cancellation.clone(),
            )
        } else {
            Encoder::create_with_codec_and_audio(
                temporary,
                info.frame_desc.clone(),
                info.frame_rate,
                OutputEncoding {
                    codec: plan.output.codec.clone(),
                    mode: encode_mode,
                },
                vaapi.clone(),
                audio_templates,
                cancellation.clone(),
            )
        };
        let encoder = encoder_result.map_err(|error| {
            let capability = if error.stage() == Some(PipelineStage::MuxInitialization) {
                InitCapability::Muxer
            } else {
                encode_capability
            };
            InitializationFailure::new(capability, error)
        })?;
        decoder.attach_audio_passthrough(audio_plan, encoder.audio_packet_sender());

        let selection = match vulkan {
            Some(backend) if plan.hardware_output_interop && plan.hardware_input_interop => {
                hooks.checkpoint(InitializationPoint::InputDrmPrimeMap, plan)?;
                hooks.checkpoint(InitializationPoint::InputDmaBufImport, plan)?;
                hooks.checkpoint(InitializationPoint::OutputVaapiFrameAcquire, plan)?;
                hooks.checkpoint(InitializationPoint::OutputDrmPrimeMap, plan)?;
                hooks.checkpoint(InitializationPoint::OutputDmaBufImport, plan)?;
                Self::require_two_slots("full interop")?;
                let device = backend.device_info().clone();
                let allocations = backend.memory_allocations();
                let frames = encoder.encoder_frames().map_err(|error| {
                    InitializationFailure::new(InitCapability::OutputInterop, error)
                })?;
                let interop = VaapiVulkanFullInteropProcessor::new(
                    backend,
                    frames,
                    info.frame_desc.clone(),
                    config.clone(),
                )
                .map_err(|error| InitializationFailure::new(InitCapability::Vulkan, error))?;
                BackendSelection {
                    processor: SelectedProcessor::FullInterop(Box::new(interop)),
                    device_info: Some(device),
                    memory_allocations: allocations,
                    mapping_strategy: Some("GPU + DMA-BUF input/output"),
                    gpu_slots: Some(2),
                }
            }
            Some(backend) if plan.hardware_output_interop => {
                hooks.checkpoint(InitializationPoint::OutputVaapiFrameAcquire, plan)?;
                hooks.checkpoint(InitializationPoint::OutputDrmPrimeMap, plan)?;
                hooks.checkpoint(InitializationPoint::OutputDmaBufImport, plan)?;
                Self::require_two_slots("output interop")?;
                let device = backend.device_info().clone();
                let allocations = backend.memory_allocations();
                let frames = encoder.encoder_frames().map_err(|error| {
                    InitializationFailure::new(InitCapability::OutputInterop, error)
                })?;
                let interop = VulkanVaapiOutputInteropProcessor::new(
                    backend,
                    frames,
                    info.frame_desc.clone(),
                    config.clone(),
                )
                .map_err(|error| {
                    InitializationFailure::new(InitCapability::OutputInterop, error)
                })?;
                BackendSelection {
                    processor: SelectedProcessor::OutputInterop(Box::new(interop)),
                    device_info: Some(device),
                    memory_allocations: allocations,
                    mapping_strategy: Some("Host input + DMA-BUF output"),
                    gpu_slots: Some(2),
                }
            }
            Some(backend) if plan.hardware_input_interop => {
                hooks.checkpoint(InitializationPoint::InputDrmPrimeMap, plan)?;
                hooks.checkpoint(InitializationPoint::InputDmaBufImport, plan)?;
                Self::require_two_slots("input interop")?;
                let device = backend.device_info().clone();
                let allocations = backend.memory_allocations();
                let interop = VaapiVulkanInteropProcessor::new(
                    backend,
                    info.frame_desc.clone(),
                    config.clone(),
                )
                .map_err(|error| InitializationFailure::new(InitCapability::InputInterop, error))?;
                BackendSelection {
                    processor: SelectedProcessor::Interop(Box::new(interop)),
                    device_info: Some(device),
                    memory_allocations: allocations,
                    mapping_strategy: Some("GPU + DMA-BUF input"),
                    gpu_slots: Some(2),
                }
            }
            Some(backend) => match args.vulkan_mapping {
                VulkanMappingArg::Cpu => {
                    let device = backend.device_info().clone();
                    let allocations = backend.memory_allocations();
                    BackendSelection {
                        processor: SelectedProcessor::Host(Box::new(HybridAsciiBackend::new(
                            backend,
                        ))),
                        device_info: Some(device),
                        memory_allocations: allocations,
                        mapping_strategy: Some("CPU"),
                        gpu_slots: Some(1),
                    }
                }
                VulkanMappingArg::Auto | VulkanMappingArg::Gpu => {
                    let slots = vulkan_frame_slots().map_err(|error| {
                        InitializationFailure::new(InitCapability::Configuration, error)
                    })?;
                    if slots == 1 {
                        let device = backend.device_info().clone();
                        let allocations = backend.memory_allocations();
                        BackendSelection {
                            processor: SelectedProcessor::Host(Box::new(backend)),
                            device_info: Some(device),
                            memory_allocations: allocations,
                            mapping_strategy: Some("GPU"),
                            gpu_slots: Some(1),
                        }
                    } else {
                        let backend = PipelinedVulkanAsciiBackend::new(
                            backend,
                            info.frame_desc.clone(),
                            config.clone(),
                        )
                        .map_err(|error| {
                            InitializationFailure::new(InitCapability::Vulkan, error)
                        })?;
                        let device = backend.device_info().clone();
                        let allocations = backend.memory_allocations();
                        BackendSelection {
                            processor: SelectedProcessor::Host(Box::new(backend)),
                            device_info: Some(device),
                            memory_allocations: allocations,
                            mapping_strategy: Some("GPU"),
                            gpu_slots: Some(2),
                        }
                    }
                }
            },
            None => BackendSelection {
                processor: SelectedProcessor::Host(Box::new(CpuAsciiBackend::with_atlas(
                    atlas.clone(),
                    config,
                ))),
                device_info: None,
                memory_allocations: Vec::new(),
                mapping_strategy: None,
                gpu_slots: None,
            },
        };

        Ok(BuiltExecution {
            decoder,
            encoder,
            selection,
        })
    }

    fn require_two_slots(path: &'static str) -> std::result::Result<(), InitializationFailure> {
        match vulkan_frame_slots() {
            Ok(2) => Ok(()),
            Ok(_) => Err(InitializationFailure::new(
                InitCapability::Configuration,
                anyhow::anyhow!("{path} requires the validated two-slot Vulkan configuration"),
            )),
            Err(error) => Err(InitializationFailure::new(
                InitCapability::Configuration,
                error,
            )),
        }
    }
}

enum SelectedProcessor {
    Host(Box<dyn AsciiBackend>),
    Interop(Box<VaapiVulkanInteropProcessor>),
    FullInterop(Box<VaapiVulkanFullInteropProcessor>),
    OutputInterop(Box<VulkanVaapiOutputInteropProcessor>),
}

impl HybridAsciiBackend {
    fn new(vulkan: VulkanAsciiBackend) -> Self {
        Self {
            mapper_key: None,
            mapper: None,
            cells: Vec::new(),
            vulkan,
        }
    }
}

impl AsciiBackend for HybridAsciiBackend {
    fn process(
        &mut self,
        input: asciiflow_core::VideoFrame,
        config: &AsciiConfig,
    ) -> asciiflow_core::Result<asciiflow_core::BackendOutput> {
        let total_started = Instant::now();
        let (grid_width, grid_height) =
            config.resolved_grid(input.desc().width, input.desc().height)?;
        if self.mapper_key.as_deref() != Some(&config.charset) {
            self.mapper = Some(Nv12Mapper::new(&config.charset)?);
            self.mapper_key = Some(config.charset.clone());
        }
        let mapping_started = Instant::now();
        let grid = self.mapper.as_ref().expect("CPU mapper initialized").map(
            &input,
            grid_width,
            grid_height,
        )?;
        self.cells.clear();
        self.cells.extend(
            grid.cells
                .iter()
                .map(|cell| asciiflow_vulkan::GpuAsciiCell {
                    glyph: cell.glyph as u32,
                    y: cell.y as u32,
                    u: cell.u as u32,
                    v: cell.v as u32,
                }),
        );
        let mapping = mapping_started.elapsed();
        let mut output =
            self.vulkan
                .render_cells(input.desc(), input.pts(), config, &self.cells)?;
        output.timings.mapping = mapping;
        output.timings.backend_wall = total_started.elapsed();
        Ok(output)
    }
}

struct LimitedSource<S> {
    inner: S,
    remaining: Option<u64>,
}
impl<S: FrameSource> FrameSource for LimitedSource<S> {
    fn next_frame(&mut self) -> asciiflow_core::Result<Option<asciiflow_core::VideoFrame>> {
        if self.remaining == Some(0) {
            return Ok(None);
        }
        let frame = self.inner.next_frame()?;
        if frame.is_some() {
            if let Some(value) = &mut self.remaining {
                *value -= 1;
            }
        }
        Ok(frame)
    }

    fn take_timings(&mut self) -> SourceTimings {
        self.inner.take_timings()
    }

    fn finish(&mut self) -> asciiflow_core::Result<()> {
        self.inner.finish()
    }
}

struct TemporaryOutputGuard {
    path: PathBuf,
    armed: bool,
}

impl TemporaryOutputGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TemporaryOutputGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn temporary_output_path(output: &Path) -> Result<PathBuf> {
    static NEXT_TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file = output
        .file_stem()
        .and_then(|v| v.to_str())
        .context("output must have a valid file name")?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = NEXT_TEMPORARY_ID.fetch_add(1, Ordering::Relaxed);
    Ok(parent.join(format!(
        ".{file}.asciiflow-part-{}-{timestamp}-{sequence}.mp4",
        std::process::id()
    )))
}

fn paths_refer_to_same_file(input: &Path, output: &Path) -> bool {
    match (fs::canonicalize(input), fs::canonicalize(output)) {
        (Ok(input), Ok(output)) => input == output,
        _ => input == output,
    }
}

fn prepare_temporary_output(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create output directory {}", parent.display()))?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("failed to reserve temporary output {}", path.display()))?;
    Ok(())
}

fn vulkan_frame_slots() -> Result<usize> {
    match std::env::var("ASCIIFLOW_VULKAN_FRAME_SLOTS").as_deref() {
        Err(_) | Ok("2") => Ok(2),
        Ok("1") => Ok(1),
        Ok(value) => bail!("ASCIIFLOW_VULKAN_FRAME_SLOTS must be 1 or 2, got {value}"),
    }
}
fn commit_output(temporary: &Path, output: &Path) -> Result<()> {
    fs::rename(temporary, output)
        .with_context(|| format!("failed to atomically commit output {}", output.display()))
}

#[cfg(test)]
mod stage13_tests {
    use super::*;
    use asciiflow_core::{ColorSpace, FrameDesc, HostFrame, VideoFrame};
    use asciiflow_vulkan::GpuAsciiCell;
    use std::time::Duration;

    fn patterned_frame() -> VideoFrame {
        let desc = FrameDesc::host_nv12(1920, 1080, ColorSpace::default()).unwrap();
        let mut bytes = vec![0_u8; desc.byte_len()];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = (index.wrapping_mul(37).wrapping_add(index / 1920 * 11) & 0xff) as u8;
        }
        VideoFrame::new_host(
            desc.clone(),
            Some(17),
            HostFrame::from_nv12(&desc, bytes).unwrap(),
        )
        .unwrap()
    }

    fn median_ms(mut values: Vec<Duration>) -> f64 {
        values.sort_unstable();
        values[values.len() / 2].as_secs_f64() * 1000.0
    }

    #[test]
    #[ignore = "real-device Stage 1.3 mapping crossover benchmark; validation must be disabled"]
    fn stage13_mapping_crossover() {
        assert!(std::env::var_os("ASCIIFLOW_VULKAN_VALIDATION").is_none());
        let input = patterned_frame();
        for name in [
            "ASCIIFLOW_VULKAN_MAP_VARIANT",
            "ASCIIFLOW_VULKAN_RENDER_VARIANT",
            "ASCIIFLOW_VULKAN_READBACK",
        ] {
            assert!(
                std::env::var_os(name).is_none(),
                "{name} would mislabel this benchmark"
            );
        }
        let device = VulkanAsciiBackend::new().unwrap();
        assert!(
            device
                .device_info()
                .name
                .contains("Intel(R) Arc(tm) Graphics (MTL)")
        );
        println!(
            "device={} frame={}x{} samples=120 warmup=20 color=true",
            device.device_info().name,
            input.desc().width,
            input.desc().height
        );
        drop(device);
        println!("width cells cpu_map_ms gpu_map_ms full_gpu_wall_ms hybrid_wall_ms winner");

        for width in [40, 80, 160, 240, 320] {
            let config = AsciiConfig {
                grid_width: width,
                grid_height: None,
                charset: "@%#*+=-:. ".into(),
                font: "builtin-8x8".into(),
                color: true,
            };
            let (grid_width, grid_height) = config
                .resolved_grid(input.desc().width, input.desc().height)
                .unwrap();
            let mapper = Nv12Mapper::new(&config.charset).unwrap();
            let reference_grid = mapper.map(&input, grid_width, grid_height).unwrap();
            let reference_cells: Vec<_> = reference_grid
                .cells
                .iter()
                .map(|cell| GpuAsciiCell {
                    glyph: cell.glyph as u32,
                    y: cell.y as u32,
                    u: cell.u as u32,
                    v: cell.v as u32,
                })
                .collect();
            let expected = CpuAsciiBackend::new()
                .process(input.clone(), &config)
                .unwrap()
                .frame;
            for _ in 0..20 {
                std::hint::black_box(mapper.map(&input, grid_width, grid_height).unwrap());
            }
            let mut cpu_map = Vec::with_capacity(120);
            for _ in 0..120 {
                let started = Instant::now();
                std::hint::black_box(mapper.map(&input, grid_width, grid_height).unwrap());
                cpu_map.push(started.elapsed());
            }
            let (gpu_map, full_wall) = {
                let mut backend = VulkanAsciiBackend::new().unwrap();
                let gpu_cells = backend.map_cells(&input, &config).unwrap();
                assert_eq!(backend.mapping_variant(), Some(("u32-32", 32)));
                assert_eq!(gpu_cells, reference_cells, "mapping parity width={width}");
                assert_eq!(
                    backend.process(input.clone(), &config).unwrap().frame,
                    expected,
                    "full GPU parity width={width}"
                );
                for _ in 0..20 {
                    std::hint::black_box(backend.process(input.clone(), &config).unwrap());
                }
                let mut gpu_map = Vec::with_capacity(120);
                let mut full_wall = Vec::with_capacity(120);
                for _ in 0..120 {
                    let output = backend.process(input.clone(), &config).unwrap();
                    gpu_map.push(output.timings.gpu_mapping);
                    full_wall.push(output.timings.backend_wall);
                }
                assert_eq!(backend.validation_error_count(), 0);
                (gpu_map, full_wall)
            };
            let hybrid_wall = {
                let mut backend = HybridAsciiBackend::new(VulkanAsciiBackend::new().unwrap());
                assert_eq!(
                    backend.process(input.clone(), &config).unwrap().frame,
                    expected,
                    "hybrid parity width={width}"
                );
                for _ in 0..20 {
                    std::hint::black_box(backend.process(input.clone(), &config).unwrap());
                }
                let mut hybrid_wall = Vec::with_capacity(120);
                for _ in 0..120 {
                    hybrid_wall.push(
                        backend
                            .process(input.clone(), &config)
                            .unwrap()
                            .timings
                            .backend_wall,
                    );
                }
                assert_eq!(backend.vulkan.validation_error_count(), 0);
                hybrid_wall
            };
            let cpu_map = median_ms(cpu_map);
            let gpu_map = median_ms(gpu_map);
            let full_wall = median_ms(full_wall);
            let hybrid_wall = median_ms(hybrid_wall);
            let winner = if hybrid_wall < full_wall {
                "hybrid"
            } else {
                "full-gpu"
            };
            println!(
                "{width} {} {cpu_map:.6} {gpu_map:.6} {full_wall:.6} {hybrid_wall:.6} {winner}",
                grid_width as u64 * grid_height as u64
            );
        }
    }
}

#[cfg(test)]
mod stage2_tests {
    use super::*;
    use asciiflow_core::FrameSource;

    #[test]
    #[ignore = "real-device Stage 2 VAAPI download and CPU/Vulkan parity"]
    fn vaapi_decode_download_and_ascii_parity() {
        let input = std::env::var("ASCIIFLOW_TEST_VIDEO")
            .expect("ASCIIFLOW_TEST_VIDEO must name the benchmark input");
        let device = std::env::var_os("ASCIIFLOW_VAAPI_DEVICE").map(PathBuf::from);
        let vaapi = VaapiOptions::new(device);
        let mut software = Decoder::open(&input).unwrap();
        let mut hardware = Decoder::open_with(&input, DecodeMode::Vaapi, vaapi).unwrap();
        assert_eq!(software.info().frame_desc, hardware.info().frame_desc);
        assert_eq!(software.info().frame_rate, hardware.info().frame_rate);

        let config = AsciiConfig {
            grid_width: 80,
            ..AsciiConfig::default()
        };
        let mut cpu = CpuAsciiBackend::new();
        let mut vulkan = VulkanAsciiBackend::new().unwrap();
        assert!(
            vulkan
                .device_info()
                .name
                .contains("Intel(R) Arc(tm) Graphics (MTL)")
        );
        let mut differing = 0_u64;
        let mut absolute_error = 0_u64;
        let mut maximum_error = 0_u8;
        for index in 0..30 {
            let software_frame = software.next_frame().unwrap().unwrap();
            let hardware_frame = hardware.next_frame().unwrap().unwrap();
            assert_eq!(software_frame.desc(), hardware_frame.desc());
            assert_eq!(software_frame.pts(), hardware_frame.pts());
            for (&left, &right) in software_frame
                .host()
                .as_slice()
                .iter()
                .zip(hardware_frame.host().as_slice())
            {
                let error = left.abs_diff(right);
                differing += u64::from(error != 0);
                absolute_error += u64::from(error);
                maximum_error = maximum_error.max(error);
            }
            let expected = cpu.process(hardware_frame.clone(), &config).unwrap().frame;
            let actual = vulkan.process(hardware_frame, &config).unwrap().frame;
            assert_eq!(actual, expected, "ASCII parity failed at frame {index}");
        }
        assert_eq!(vulkan.validation_error_count(), 0);
        println!(
            "frames=30 decoded_differing_bytes={differing} decoded_absolute_error={absolute_error} decoded_max_error={maximum_error}"
        );
    }
}

#[cfg(test)]
mod stage40_tests {
    use super::*;
    use asciiflow_core::{
        ChromaSubsampling, ColorSpace, InputRequirements, InteropCapabilities, MediaCapabilities,
        PixelPath, ProcessingCapabilities, Rational, VideoCodec, VulkanDeviceKind,
    };

    fn supported() -> CapabilitySupport {
        CapabilitySupport::Supported
    }

    fn full_capabilities() -> CapabilitySnapshot {
        CapabilitySnapshot {
            media: MediaCapabilities {
                software_decode: supported(),
                software_encode: supported(),
                vaapi_device: supported(),
                h264_vaapi_decode: supported(),
                hevc_vaapi_decode: supported(),
                av1_vaapi_decode: supported(),
                h264_vaapi_encode: supported(),
                hevc_vaapi_encode: supported(),
                hevc_main10_vaapi_decode: supported(),
                av1_10bit_vaapi_decode: supported(),
                hevc_main10_vaapi_encode: supported(),
                av1_vaapi_encode: supported(),
                av1_10bit_vaapi_encode: supported(),
                nv12_hardware_frames: supported(),
                nv12_hardware_upload: supported(),
                p010_hardware_frames: supported(),
                p010_hardware_upload: supported(),
            },
            processing: ProcessingCapabilities {
                cpu: supported(),
                vulkan: supported(),
                vulkan_auto_eligible: true,
                vulkan_device_name: Some("synthetic GPU".into()),
                vulkan_device_kind: Some(VulkanDeviceKind::IntegratedGpu),
                compute_queue: supported(),
                storage_buffer_8bit: supported(),
                shader_int64: supported(),
                synchronization2: supported(),
            },
            interop: InteropCapabilities {
                input: supported(),
                hevc_input: supported(),
                av1_input: supported(),
                output: supported(),
                hevc_output: supported(),
                av1_output: supported(),
                p010_input: supported(),
                p010_output: supported(),
                av1_p010_output: supported(),
            },
        }
    }

    fn requirements() -> InputRequirements {
        InputRequirements {
            codec: VideoCodec::H264,
            profile: Some("High".into()),
            pixel_format: Some("yuv420p".into()),
            bit_depth: Some(8),
            chroma_subsampling: ChromaSubsampling::Yuv420,
            width: 1920,
            height: 1080,
            frame_rate: Rational::new(50, 1).unwrap(),
            color_space: ColorSpace::default(),
            color_semantics: None,
        }
    }

    fn injected(capability: InitCapability, message: &'static str) -> InitializationFailure {
        InitializationFailure::new(capability, anyhow::anyhow!(message))
    }

    #[test]
    fn temporary_output_names_are_unique_and_support_bare_file_names() {
        let first = temporary_output_path(Path::new("output.mp4")).unwrap();
        let second = temporary_output_path(Path::new("output.mp4")).unwrap();
        assert_ne!(first, second);
        assert_eq!(first.parent(), Some(Path::new(".")));
    }

    #[test]
    fn existing_path_aliases_cannot_overwrite_the_input() {
        let input = std::env::current_dir().unwrap().join("Cargo.toml");
        assert!(paths_refer_to_same_file(&input, Path::new("./Cargo.toml")));
    }

    #[test]
    fn failed_or_cancelled_staging_preserves_existing_destination() {
        let root = std::env::temp_dir().join(format!(
            "asciiflow-stage41-output-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let destination = root.join("output.mp4");
        fs::write(&destination, b"known-existing-output").unwrap();
        let temporary = temporary_output_path(&destination).unwrap();
        prepare_temporary_output(&temporary).unwrap();
        fs::write(&temporary, b"partial-corrupt-output").unwrap();
        {
            let _guard = TemporaryOutputGuard::new(temporary.clone());
        }
        assert_eq!(fs::read(&destination).unwrap(), b"known-existing-output");
        assert!(!temporary.exists());
        assert!(fs::read_dir(&root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("asciiflow-part")
        }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn successful_commit_replaces_destination_and_removes_staging() {
        let root = std::env::temp_dir().join(format!(
            "asciiflow-stage41-success-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let destination = root.join("output.mp4");
        fs::write(&destination, b"old").unwrap();
        let temporary = temporary_output_path(&destination).unwrap();
        prepare_temporary_output(&temporary).unwrap();
        fs::write(&temporary, b"complete-new-output").unwrap();
        let mut guard = TemporaryOutputGuard::new(temporary.clone());
        commit_output(&temporary, &destination).unwrap();
        guard.disarm();
        assert_eq!(fs::read(&destination).unwrap(), b"complete-new-output");
        assert!(!temporary.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn only_auto_policy_fields_are_eligible_for_initialization_replan() {
        let automatic = PipelinePolicy::default();
        assert!(InitCapability::HardwareDecode.is_auto(&automatic));
        assert!(InitCapability::HardwareEncode.is_auto(&automatic));
        assert!(InitCapability::Vulkan.is_auto(&automatic));
        assert!(InitCapability::InputInterop.is_auto(&automatic));
        assert!(InitCapability::OutputInterop.is_auto(&automatic));

        let explicit = PipelinePolicy {
            backend: ProcessingBackend::Vulkan,
            decode: asciiflow_core::MediaRequest::Hardware,
            encode: asciiflow_core::MediaRequest::Hardware,
            input_interop: asciiflow_core::InteropRequest::On,
            output_interop: asciiflow_core::InteropRequest::On,
            output_codec: asciiflow_core::VideoCodec::H264,
            output_bit_depth: 8,
        };
        assert!(!InitCapability::HardwareDecode.is_auto(&explicit));
        assert!(!InitCapability::HardwareEncode.is_auto(&explicit));
        assert!(!InitCapability::Vulkan.is_auto(&explicit));
        assert!(!InitCapability::InputInterop.is_auto(&explicit));
        assert!(!InitCapability::OutputInterop.is_auto(&explicit));
    }

    #[test]
    fn cooperative_cancellation_maps_to_sigint_exit_contract() {
        let error = anyhow::Error::new(asciiflow_core::Error::Cancelled);
        assert!(is_cancellation(&error));
        assert_eq!(failure_exit_code(&error), 130);
    }

    #[test]
    fn every_initialization_fault_point_is_deterministic_and_single_shot() {
        let plan = PipelinePlanner::select(
            &full_capabilities(),
            &requirements(),
            PipelinePolicy::default(),
        )
        .unwrap()
        .selected;
        let points = [
            InitializationPoint::DecoderCreate,
            InitializationPoint::VaapiFramesPoolCreate,
            InitializationPoint::InputDrmPrimeMap,
            InitializationPoint::InputDmaBufImport,
            InitializationPoint::VulkanProcessorCreate,
            InitializationPoint::OutputVaapiFrameAcquire,
            InitializationPoint::OutputDrmPrimeMap,
            InitializationPoint::OutputDmaBufImport,
            InitializationPoint::EncoderCreate,
            InitializationPoint::MuxerCreate,
        ];
        for point in points {
            let mut injector = InitializationFaultInjector::new(point);
            let first = injector.checkpoint(point, &plan).unwrap_err();
            assert!(first.to_string().contains(point.operation()));
            assert!(injector.checkpoint(point, &plan).is_ok());
        }
    }

    #[test]
    fn input_interop_failure_replans_to_software_decode_without_hwdownload() {
        let policy = PipelinePolicy::default();
        let requirements = requirements();
        let mut snapshot = full_capabilities();
        let initial = PipelinePlanner::select(&snapshot, &requirements, policy.clone()).unwrap();
        let mut attempt = 0;
        let initialized = initialize_with_replan(
            &mut snapshot,
            &requirements,
            policy,
            initial,
            |plan| {
                attempt += 1;
                if attempt == 1 {
                    Err(injected(
                        InitCapability::InputInterop,
                        "injected input DMA-BUF import failure",
                    ))
                } else {
                    Ok(plan.clone())
                }
            },
            || Ok(()),
        )
        .unwrap();
        assert_eq!(attempt, 2);
        assert_eq!(initialized.value.decode, MediaImplementation::Software);
        assert!(!initialized.value.hardware_download);
        assert!(initialized.value.hardware_output_interop);
        assert!(snapshot.interop.input.unavailable_reason().is_some());
        assert!(snapshot.interop.output.is_supported());
    }

    #[test]
    fn output_interop_failure_keeps_vaapi_encode_via_host_upload() {
        let policy = PipelinePolicy::default();
        let requirements = requirements();
        let mut snapshot = full_capabilities();
        let initial = PipelinePlanner::select(&snapshot, &requirements, policy.clone()).unwrap();
        let mut attempt = 0;
        let initialized = initialize_with_replan(
            &mut snapshot,
            &requirements,
            policy,
            initial,
            |plan| {
                attempt += 1;
                if attempt == 1 {
                    Err(injected(
                        InitCapability::OutputInterop,
                        "injected output DMA-BUF import failure",
                    ))
                } else {
                    Ok(plan.clone())
                }
            },
            || Ok(()),
        )
        .unwrap();
        assert_eq!(initialized.value.pixel_path, PixelPath::PartiallyStaged);
        assert!(initialized.value.hardware_input_interop);
        assert!(initialized.value.hardware_upload);
        assert_eq!(initialized.value.encode, MediaImplementation::Hardware);
        assert!(snapshot.interop.input.is_supported());
        assert!(snapshot.media.h264_vaapi_encode.is_supported());
    }

    #[test]
    fn hevc_output_interop_failure_replans_only_hevc_to_staged_encode() {
        let policy = PipelinePolicy {
            output_codec: VideoCodec::Hevc,
            ..Default::default()
        };
        let requirements = requirements();
        let mut snapshot = full_capabilities();
        let initial = PipelinePlanner::select(&snapshot, &requirements, policy.clone()).unwrap();
        let mut attempt = 0;
        let initialized = initialize_with_replan(
            &mut snapshot,
            &requirements,
            policy,
            initial,
            |plan| {
                attempt += 1;
                if attempt == 1 {
                    Err(injected(
                        InitCapability::OutputInterop,
                        "injected HEVC output DMA-BUF import failure",
                    ))
                } else {
                    Ok(plan.clone())
                }
            },
            || Ok(()),
        )
        .unwrap();
        assert_eq!(attempt, 2);
        assert_eq!(initialized.value.output.codec, VideoCodec::Hevc);
        assert!(initialized.value.hardware_upload);
        assert!(!initialized.value.hardware_output_interop);
        assert!(snapshot.interop.output.is_supported());
        assert!(!snapshot.interop.hevc_output.is_supported());
        assert!(snapshot.media.hevc_vaapi_encode.is_supported());
    }

    #[test]
    fn av1_output_interop_failure_replans_only_av1_to_staged_encode() {
        let policy = PipelinePolicy {
            output_codec: VideoCodec::Av1,
            ..Default::default()
        };
        let requirements = requirements();
        let mut snapshot = full_capabilities();
        let initial = PipelinePlanner::select(&snapshot, &requirements, policy.clone()).unwrap();
        let mut attempts = 0;
        let initialized = initialize_with_replan(
            &mut snapshot,
            &requirements,
            policy,
            initial,
            |plan| {
                attempts += 1;
                if attempts == 1 {
                    Err(injected(
                        InitCapability::OutputInterop,
                        "injected AV1 output DMA-BUF import failure",
                    ))
                } else {
                    Ok(plan.clone())
                }
            },
            || Ok(()),
        )
        .unwrap();
        assert_eq!(attempts, 2);
        assert_eq!(initialized.value.output.codec, VideoCodec::Av1);
        assert!(initialized.value.hardware_upload);
        assert!(!initialized.value.hardware_output_interop);
        assert!(!snapshot.interop.av1_output.is_supported());
        assert!(snapshot.interop.output.is_supported());
        assert!(snapshot.interop.hevc_output.is_supported());
        assert!(snapshot.media.av1_vaapi_encode.is_supported());
    }

    fn main10_requirements() -> asciiflow_core::InputRequirements {
        let mut input = requirements();
        input.codec = VideoCodec::Hevc;
        input.profile = Some(asciiflow_core::VideoProfile::HevcMain10);
        input.pixel_format = Some("yuv420p10le".into());
        input.bit_depth = Some(10);
        input
    }

    #[test]
    fn main10_output_interop_init_failure_replans_to_p010_staged_only() {
        let policy = PipelinePolicy {
            output_codec: VideoCodec::Hevc,
            output_bit_depth: 10,
            ..Default::default()
        };
        let requirements = main10_requirements();
        for point in [
            InitializationPoint::OutputVaapiFrameAcquire,
            InitializationPoint::OutputDrmPrimeMap,
            InitializationPoint::OutputDmaBufImport,
        ] {
            let mut snapshot = full_capabilities();
            let initial =
                PipelinePlanner::select(&snapshot, &requirements, policy.clone()).unwrap();
            assert!(initial.selected.hardware_output_interop);
            let capability = point.capability(&initial.selected);
            let mut attempts = 0;
            let initialized = initialize_with_replan(
                &mut snapshot,
                &requirements,
                policy.clone(),
                initial,
                |plan| {
                    attempts += 1;
                    if attempts == 1 {
                        Err(injected(
                            capability,
                            "injected P010 output initialization failure",
                        ))
                    } else {
                        Ok(plan.clone())
                    }
                },
                || Ok(()),
            )
            .unwrap();
            assert_eq!(attempts, 2);
            assert_eq!(
                initialized.value.output.profile,
                Some(asciiflow_core::VideoProfile::HevcMain10)
            );
            assert_eq!(
                initialized.value.output.pixel_format,
                asciiflow_core::PixelFormat::P010Le
            );
            assert!(initialized.value.hardware_upload);
            assert!(!initialized.value.hardware_output_interop);
            assert!(!snapshot.interop.p010_output.is_supported());
            assert!(snapshot.interop.hevc_output.is_supported());
        }
    }

    #[test]
    fn main10_encoder_init_failure_is_terminal_and_does_not_disable_main8() {
        let policy = PipelinePolicy {
            output_codec: VideoCodec::Hevc,
            output_bit_depth: 10,
            ..Default::default()
        };
        let requirements = main10_requirements();
        for point in [
            InitializationPoint::EncoderCreate,
            InitializationPoint::VaapiFramesPoolCreate,
        ] {
            let mut snapshot = full_capabilities();
            let initial =
                PipelinePlanner::select(&snapshot, &requirements, policy.clone()).unwrap();
            let capability = point.capability(&initial.selected);
            let error = initialize_with_replan(
                &mut snapshot,
                &requirements,
                policy.clone(),
                initial,
                |_| Err::<(), _>(injected(capability, "injected Main10 encoder/pool failure")),
                || Ok(()),
            )
            .unwrap_err();
            assert!(error.to_string().contains("Main10 encoder/pool failure"));
            assert!(!snapshot.media.hevc_main10_vaapi_encode.is_supported());
            assert!(snapshot.media.hevc_vaapi_encode.is_supported());
            assert!(snapshot.media.h264_vaapi_encode.is_supported());
        }
    }

    #[test]
    fn av1_10bit_initialization_failures_preserve_codec_and_main8_facts() {
        let policy = PipelinePolicy {
            output_codec: VideoCodec::Av1,
            output_bit_depth: 10,
            ..Default::default()
        };
        let requirements = main10_requirements();
        for point in [
            InitializationPoint::OutputVaapiFrameAcquire,
            InitializationPoint::OutputDrmPrimeMap,
            InitializationPoint::OutputDmaBufImport,
        ] {
            let mut snapshot = full_capabilities();
            let initial =
                PipelinePlanner::select(&snapshot, &requirements, policy.clone()).unwrap();
            assert!(initial.selected.hardware_output_interop);
            let capability = point.capability(&initial.selected);
            let mut attempts = 0;
            let initialized = initialize_with_replan(
                &mut snapshot,
                &requirements,
                policy.clone(),
                initial,
                |plan| {
                    attempts += 1;
                    if attempts == 1 {
                        Err(injected(capability, "injected AV1 P010 interop failure"))
                    } else {
                        Ok(plan.clone())
                    }
                },
                || Ok(()),
            )
            .unwrap();
            assert_eq!(attempts, 2);
            assert_eq!(initialized.value.output.codec, VideoCodec::Av1);
            assert_eq!(initialized.value.output.bit_depth, 10);
            assert!(initialized.value.hardware_upload);
            assert!(!snapshot.interop.av1_p010_output.is_supported());
            assert!(snapshot.interop.p010_output.is_supported());
        }

        for point in [
            InitializationPoint::EncoderCreate,
            InitializationPoint::VaapiFramesPoolCreate,
        ] {
            let mut snapshot = full_capabilities();
            let initial =
                PipelinePlanner::select(&snapshot, &requirements, policy.clone()).unwrap();
            let capability = point.capability(&initial.selected);
            let error = initialize_with_replan(
                &mut snapshot,
                &requirements,
                policy.clone(),
                initial,
                |_| Err::<(), _>(injected(capability, "injected AV1 10-bit encoder failure")),
                || Ok(()),
            )
            .unwrap_err();
            assert!(error.to_string().contains("AV1 10-bit encoder failure"));
            assert!(!snapshot.media.av1_10bit_vaapi_encode.is_supported());
            assert!(snapshot.media.av1_vaapi_encode.is_supported());
            assert!(snapshot.media.hevc_main10_vaapi_encode.is_supported());
        }
    }

    #[test]
    fn hevc_encoder_or_frames_pool_failure_never_changes_output_codec() {
        for capability in [
            InitCapability::HardwareEncode,
            InitializationPoint::VaapiFramesPoolCreate.capability(
                &PipelinePlanner::select(
                    &full_capabilities(),
                    &requirements(),
                    PipelinePolicy {
                        output_codec: VideoCodec::Hevc,
                        ..Default::default()
                    },
                )
                .unwrap()
                .selected,
            ),
        ] {
            let policy = PipelinePolicy {
                output_codec: VideoCodec::Hevc,
                ..Default::default()
            };
            let requirements = requirements();
            let mut snapshot = full_capabilities();
            let initial =
                PipelinePlanner::select(&snapshot, &requirements, policy.clone()).unwrap();
            let error = initialize_with_replan(
                &mut snapshot,
                &requirements,
                policy,
                initial,
                |_| {
                    Err::<(), _>(injected(
                        capability,
                        "injected HEVC encoder initialization failure",
                    ))
                },
                || Ok(()),
            )
            .unwrap_err();
            assert!(error.to_string().contains("HEVC"));
            assert!(snapshot.media.h264_vaapi_encode.is_supported());
            assert!(!snapshot.media.hevc_vaapi_encode.is_supported());
        }
    }

    #[test]
    fn av1_encoder_or_frames_pool_failure_is_terminal_without_codec_fallback() {
        let policy = PipelinePolicy {
            output_codec: VideoCodec::Av1,
            ..Default::default()
        };
        for capability in [
            InitCapability::HardwareEncode,
            InitializationPoint::VaapiFramesPoolCreate.capability(
                &PipelinePlanner::select(&full_capabilities(), &requirements(), policy.clone())
                    .unwrap()
                    .selected,
            ),
        ] {
            let requirements = requirements();
            let mut snapshot = full_capabilities();
            let initial =
                PipelinePlanner::select(&snapshot, &requirements, policy.clone()).unwrap();
            let error = initialize_with_replan(
                &mut snapshot,
                &requirements,
                policy.clone(),
                initial,
                |_| {
                    Err::<(), _>(injected(
                        capability,
                        "injected AV1 encoder initialization failure",
                    ))
                },
                || Ok(()),
            )
            .unwrap_err();
            assert!(error.to_string().contains("AV1"));
            assert!(!snapshot.media.av1_vaapi_encode.is_supported());
            assert!(snapshot.media.h264_vaapi_encode.is_supported());
            assert!(snapshot.media.hevc_vaapi_encode.is_supported());
        }
    }

    #[test]
    fn new_codec_input_replan_leaves_other_codecs_available() {
        for (codec, profile) in [
            (VideoCodec::Hevc, asciiflow_core::VideoProfile::HevcMain),
            (VideoCodec::Av1, asciiflow_core::VideoProfile::Av1Main),
        ] {
            let mut req = requirements();
            req.codec = codec.clone();
            req.profile = Some(profile);
            let mut snapshot = full_capabilities();
            let initial = PipelinePlanner::select(&snapshot, &req, Default::default()).unwrap();
            let mut attempts = 0;
            let result = initialize_with_replan(
                &mut snapshot,
                &req,
                Default::default(),
                initial,
                |plan| {
                    attempts += 1;
                    if attempts == 1 {
                        Err(injected(
                            InitCapability::InputInterop,
                            "new codec input import failed",
                        ))
                    } else {
                        Ok(plan.clone())
                    }
                },
                || Ok(()),
            )
            .unwrap();
            assert_eq!(attempts, 2);
            assert_eq!(result.value.decode, MediaImplementation::Software);
            assert!(!snapshot.interop.input_for(&codec).is_supported());
            assert!(snapshot.interop.input.is_supported());
            assert!(snapshot.media.decode_for(&codec).is_supported());
        }
    }

    #[test]
    fn automatic_decoder_vulkan_and_encoder_failures_replan_once() {
        type PlanCheck = fn(&PipelinePlan) -> bool;
        let cases: [(InitCapability, PlanCheck); 3] = [
            (InitCapability::HardwareDecode, |plan: &PipelinePlan| {
                plan.decode == MediaImplementation::Software
            }),
            (InitCapability::Vulkan, |plan: &PipelinePlan| {
                plan.backend == ProcessingBackend::Cpu
            }),
            (InitCapability::HardwareEncode, |plan: &PipelinePlan| {
                plan.encode == MediaImplementation::Software
            }),
        ];
        for (failure, verify) in cases {
            let policy = PipelinePolicy::default();
            let requirements = requirements();
            let mut snapshot = full_capabilities();
            let initial =
                PipelinePlanner::select(&snapshot, &requirements, policy.clone()).unwrap();
            let mut attempt = 0;
            let initialized = initialize_with_replan(
                &mut snapshot,
                &requirements,
                policy,
                initial,
                |plan| {
                    attempt += 1;
                    if attempt == 1 {
                        Err(injected(failure, "injected initialization failure"))
                    } else {
                        Ok(plan.clone())
                    }
                },
                || Ok(()),
            )
            .unwrap();
            assert_eq!(attempt, 2);
            assert!(verify(&initialized.value));
        }
    }

    #[test]
    fn explicit_initialization_failure_never_replans() {
        let policy = PipelinePolicy {
            decode: asciiflow_core::MediaRequest::Hardware,
            ..PipelinePolicy::default()
        };
        let requirements = requirements();
        let mut snapshot = full_capabilities();
        let initial = PipelinePlanner::select(&snapshot, &requirements, policy.clone()).unwrap();
        let mut attempts = 0;
        let error = initialize_with_replan(
            &mut snapshot,
            &requirements,
            policy,
            initial,
            |_| {
                attempts += 1;
                Err::<(), _>(injected(
                    InitCapability::HardwareDecode,
                    "injected explicit decoder failure",
                ))
            },
            || Ok(()),
        )
        .unwrap_err();
        assert_eq!(attempts, 1);
        assert!(error.to_string().contains("decoder initialization"));
    }

    #[test]
    fn second_initialization_failure_is_terminal_and_explains_both_plans() {
        let policy = PipelinePolicy::default();
        let requirements = requirements();
        let mut snapshot = full_capabilities();
        let initial = PipelinePlanner::select(&snapshot, &requirements, policy.clone()).unwrap();
        let mut attempts = 0;
        let error = initialize_with_replan(
            &mut snapshot,
            &requirements,
            policy,
            initial,
            |_| {
                attempts += 1;
                Err::<(), _>(if attempts == 1 {
                    injected(InitCapability::InputInterop, "first injected failure")
                } else {
                    injected(InitCapability::SoftwareDecode, "second injected failure")
                })
            },
            || Ok(()),
        )
        .unwrap_err();
        assert_eq!(attempts, 2);
        let message = error.to_string();
        assert!(message.contains("initial plan:"));
        assert!(message.contains("first injected failure"));
        assert!(message.contains("replanned plan:"));
        assert!(message.contains("second injected failure"));
    }

    #[test]
    #[ignore = "real-device Stage 4 repeated capability probe FD lifetime test"]
    fn repeated_capability_probe_releases_file_descriptors() {
        let input = std::env::var("ASCIIFLOW_TEST_VIDEO")
            .expect("ASCIIFLOW_TEST_VIDEO must name the benchmark input");
        let device = std::env::var_os("ASCIIFLOW_VAAPI_DEVICE").map(PathBuf::from);
        let vaapi = VaapiOptions::new(device);

        drop(capabilities::probe(Path::new(&input), &vaapi, &AsciiConfig::default()).unwrap());
        let before = std::fs::read_dir("/proc/self/fd").unwrap().count();
        for _ in 0..100 {
            let probe =
                capabilities::probe(Path::new(&input), &vaapi, &AsciiConfig::default()).unwrap();
            assert!(probe.snapshot.processing.vulkan.is_supported());
            drop(probe);
        }
        let after = std::fs::read_dir("/proc/self/fd").unwrap().count();
        assert_eq!(after, before, "capability probe leaked file descriptors");
    }
}
