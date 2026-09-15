use asciiflow_core::{AudioPolicy, InteropRequest, MediaRequest, ProcessingBackend};
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum BackendArg {
    Auto,
    Cpu,
    Vulkan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum VulkanMappingArg {
    Auto,
    Gpu,
    Cpu,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum MediaArg {
    Auto,
    Software,
    Vaapi,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum InteropArg {
    Auto,
    Off,
    On,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum AudioArg {
    Auto,
    Copy,
    None,
}

impl From<AudioArg> for AudioPolicy {
    fn from(value: AudioArg) -> Self {
        match value {
            AudioArg::Auto => Self::Auto,
            AudioArg::Copy => Self::Copy,
            AudioArg::None => Self::None,
        }
    }
}

impl From<MediaArg> for MediaRequest {
    fn from(value: MediaArg) -> Self {
        match value {
            MediaArg::Auto => Self::Auto,
            MediaArg::Software => Self::Software,
            MediaArg::Vaapi => Self::Hardware,
        }
    }
}
impl From<BackendArg> for ProcessingBackend {
    fn from(value: BackendArg) -> Self {
        match value {
            BackendArg::Auto => Self::Auto,
            BackendArg::Cpu => Self::Cpu,
            BackendArg::Vulkan => Self::Vulkan,
        }
    }
}

impl From<InteropArg> for InteropRequest {
    fn from(value: InteropArg) -> Self {
        match value {
            InteropArg::Auto => Self::Auto,
            InteropArg::Off => Self::Off,
            InteropArg::On => Self::On,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "asciiflow",
    version,
    about = "AsciiFlow v2: bounded NV12 ASCII video pipeline"
)]
pub struct Args {
    pub input: PathBuf,
    pub output: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "auto")]
    pub backend: BackendArg,
    #[arg(long, value_enum, default_value = "auto")]
    pub vulkan_mapping: VulkanMappingArg,
    #[arg(long, value_enum, default_value = "auto")]
    pub decode: MediaArg,
    #[arg(long, value_enum, default_value = "auto")]
    pub encode: MediaArg,
    #[arg(long, value_enum, default_value = "auto")]
    pub audio: AudioArg,
    #[arg(long)]
    pub hw_device: Option<PathBuf>,
    #[arg(
        long = "vaapi-vulkan-input-interop",
        alias = "vaapi-vulkan-interop",
        alias = "input-interop",
        value_enum,
        default_value = "auto"
    )]
    pub vaapi_vulkan_input_interop: InteropArg,
    #[arg(long, alias = "output-interop", value_enum, default_value = "auto")]
    pub vaapi_vulkan_output_interop: InteropArg,
    #[arg(long)]
    pub explain_plan: bool,
    #[arg(long)]
    pub capabilities: bool,
    #[arg(long,default_value_t=160,value_parser=clap::value_parser!(u32).range(1..=8192))]
    pub width: u32,
    #[arg(long,value_parser=clap::value_parser!(u32).range(1..=8192))]
    pub height: Option<u32>,
    #[arg(long, default_value = "standard")]
    pub charset: String,
    #[arg(long, default_value = "builtin-8x8")]
    pub font: String,
    #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u32).range(0..=i32::MAX as i64))]
    pub font_face_index: u32,
    #[arg(long,default_value_t=true,action=clap::ArgAction::Set)]
    pub color: bool,
    #[arg(long, default_value_t = 0)]
    pub max_frames: u64,
    #[arg(long)]
    pub no_progress: bool,
    #[arg(short, long)]
    pub verbose: bool,
}

impl Args {
    pub fn resolved_charset(&self) -> String {
        match self.charset.as_str() {
            "standard" => "@%#*+=-:. ".into(),
            "detailed" => {
                "$@B%8&WM#*oahkbdpqwmZO0QLCJUYXzcvunxrjft/\\|()1{}[]?-_+~<>i!lI;:,\"^`'. ".into()
            }
            literal => literal.into(),
        }
    }
}
