use asciiflow_core::{
    AudioPolicy, InteropRequest, MediaRequest, OutputDynamicRange, ProcessingBackend,
    STANDARD_CHARSET, VideoCodec,
};
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum OutputCodecArg {
    #[default]
    H264,
    Hevc,
    Av1,
}

impl From<OutputCodecArg> for VideoCodec {
    fn from(value: OutputCodecArg) -> Self {
        match value {
            OutputCodecArg::H264 => Self::H264,
            OutputCodecArg::Hevc => Self::Hevc,
            OutputCodecArg::Av1 => Self::Av1,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum OutputBitDepthArg {
    #[default]
    #[value(name = "8")]
    Eight,
    #[value(name = "10")]
    Ten,
}

impl From<OutputBitDepthArg> for u8 {
    fn from(value: OutputBitDepthArg) -> Self {
        match value {
            OutputBitDepthArg::Eight => 8,
            OutputBitDepthArg::Ten => 10,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum OutputDynamicRangeArg {
    #[default]
    Preserve,
    Sdr,
}

impl From<OutputDynamicRangeArg> for OutputDynamicRange {
    fn from(value: OutputDynamicRangeArg) -> Self {
        match value {
            OutputDynamicRangeArg::Preserve => Self::Preserve,
            OutputDynamicRangeArg::Sdr => Self::Sdr,
        }
    }
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
    about = "Bounded NV12/P010 ASCII video pipeline"
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
    #[arg(long, value_enum, default_value = "h264")]
    pub output_codec: OutputCodecArg,
    #[arg(long, value_enum, default_value = "8")]
    pub output_bit_depth: OutputBitDepthArg,
    #[arg(
        long,
        value_enum,
        default_value = "preserve",
        help = "Output dynamic range: preserve the input signal or convert HDR PQ to SDR"
    )]
    pub output_dynamic_range: OutputDynamicRangeArg,
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
    #[arg(
        long,
        value_name = "PATH",
        help = "Write structured diagnostics as JSON; refuses to overwrite an existing file"
    )]
    pub diagnostic_report: Option<PathBuf>,
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
            "standard" => STANDARD_CHARSET.into(),
            "detailed" => {
                // The legacy spelling is dense-to-sparse; reverse it so the
                // shared dark-to-light LUT selects sparse-to-dense glyphs.
                "$@B%8&WM#*oahkbdpqwmZO0QLCJUYXzcvunxrjft/\\|()1{}[]?-_+~<>i!lI;:,\"^`'. "
                    .chars()
                    .rev()
                    .collect()
            }
            literal => literal.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_codec_defaults_to_h264_and_parses_hardware_codecs_independently() {
        let default = Args::try_parse_from(["asciiflow", "input.mp4", "output.mp4"]).unwrap();
        assert!(default.diagnostic_report.is_none());
        assert_eq!(default.output_codec, OutputCodecArg::H264);
        assert_eq!(default.output_bit_depth, OutputBitDepthArg::Eight);
        assert_eq!(
            default.output_dynamic_range,
            OutputDynamicRangeArg::Preserve
        );
        assert_eq!(
            OutputDynamicRange::from(default.output_dynamic_range),
            OutputDynamicRange::Preserve
        );
        assert_eq!(default.encode, MediaArg::Auto);

        let hevc = Args::try_parse_from([
            "asciiflow",
            "input.mp4",
            "output.mp4",
            "--output-codec",
            "hevc",
            "--encode",
            "vaapi",
        ])
        .unwrap();
        assert_eq!(hevc.output_codec, OutputCodecArg::Hevc);
        assert_eq!(hevc.output_bit_depth, OutputBitDepthArg::Eight);
        assert_eq!(hevc.encode, MediaArg::Vaapi);

        let av1 = Args::try_parse_from([
            "asciiflow",
            "input.mp4",
            "output.mp4",
            "--output-codec",
            "av1",
            "--encode",
            "vaapi",
        ])
        .unwrap();
        assert_eq!(av1.output_codec, OutputCodecArg::Av1);
        assert_eq!(av1.output_bit_depth, OutputBitDepthArg::Eight);
        assert_eq!(av1.encode, MediaArg::Vaapi);

        let av1_ten = Args::try_parse_from([
            "asciiflow",
            "input.mp4",
            "output.mp4",
            "--output-codec",
            "av1",
            "--output-bit-depth",
            "10",
        ])
        .unwrap();
        assert_eq!(av1_ten.output_codec, OutputCodecArg::Av1);
        assert_eq!(av1_ten.output_bit_depth, OutputBitDepthArg::Ten);

        let main10 = Args::try_parse_from([
            "asciiflow",
            "input.mp4",
            "output.mp4",
            "--output-codec",
            "hevc",
            "--output-bit-depth",
            "10",
        ])
        .unwrap();
        assert_eq!(main10.output_bit_depth, OutputBitDepthArg::Ten);
        assert!(
            Args::try_parse_from([
                "asciiflow",
                "input.mp4",
                "output.mp4",
                "--output-bit-depth",
                "12",
            ])
            .is_err()
        );
    }

    #[test]
    fn diagnostic_report_is_optional_and_help_documents_overwrite_policy() {
        use clap::CommandFactory;
        let args = Args::try_parse_from([
            "asciiflow",
            "input.mp4",
            "--explain-plan",
            "--diagnostic-report",
            "report.json",
        ])
        .unwrap();
        assert_eq!(
            args.diagnostic_report.as_deref(),
            Some(std::path::Path::new("report.json"))
        );
        assert!(args.output.is_none());
        let help = Args::command().render_long_help().to_string();
        assert!(help.contains("--diagnostic-report <PATH>"));
        assert!(help.contains("refuses to overwrite"));
    }

    #[test]
    fn output_dynamic_range_accepts_preserve_and_sdr_and_rejects_other_values() {
        let preserve = Args::try_parse_from([
            "asciiflow",
            "input.mp4",
            "output.mp4",
            "--output-dynamic-range",
            "preserve",
        ])
        .unwrap();
        assert_eq!(
            preserve.output_dynamic_range,
            OutputDynamicRangeArg::Preserve
        );
        assert_eq!(
            OutputDynamicRange::from(preserve.output_dynamic_range),
            OutputDynamicRange::Preserve
        );

        let sdr = Args::try_parse_from([
            "asciiflow",
            "input.mp4",
            "output.mp4",
            "--output-dynamic-range",
            "sdr",
        ])
        .unwrap();
        assert_eq!(sdr.output_dynamic_range, OutputDynamicRangeArg::Sdr);
        assert_eq!(
            OutputDynamicRange::from(sdr.output_dynamic_range),
            OutputDynamicRange::Sdr
        );

        assert!(
            Args::try_parse_from([
                "asciiflow",
                "input.mp4",
                "output.mp4",
                "--output-dynamic-range",
                "auto",
            ])
            .is_err()
        );
    }

    #[test]
    fn built_in_ramps_are_sparse_to_dense_on_a_black_background() {
        let default = Args::try_parse_from(["asciiflow", "input.mp4", "output.mp4"]).unwrap();
        assert_eq!(default.resolved_charset(), STANDARD_CHARSET);
        assert_eq!(
            default.resolved_charset(),
            asciiflow_core::AsciiConfig::default().charset
        );

        let detailed = Args::try_parse_from([
            "asciiflow",
            "input.mp4",
            "output.mp4",
            "--charset",
            "detailed",
        ])
        .unwrap();
        let ramp = detailed.resolved_charset();
        assert_eq!(ramp.chars().next(), Some(' '));
        assert_eq!(ramp.chars().last(), Some('$'));
    }
}
