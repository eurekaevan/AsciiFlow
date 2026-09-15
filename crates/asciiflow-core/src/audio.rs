#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioPolicy {
    Auto,
    Copy,
    None,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioStreamInfo {
    pub input_index: usize,
    pub codec_id: i32,
    pub codec: String,
    pub profile: Option<String>,
    pub time_base: crate::Rational,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub bit_rate: Option<u64>,
    pub language: Option<String>,
    pub start_time: Option<i64>,
    pub default: bool,
    pub forced: bool,
    pub mp4_compatible: bool,
    pub compatibility_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioStreamPlan {
    pub input_index: usize,
    pub codec: String,
    pub language: Option<String>,
    pub default: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkippedAudioStream {
    pub input_index: usize,
    pub codec: String,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioPlan {
    pub policy: AudioPolicy,
    pub selected: Vec<AudioStreamPlan>,
    pub skipped: Vec<SkippedAudioStream>,
}

impl AudioPlan {
    pub fn select(policy: AudioPolicy, streams: &[AudioStreamInfo]) -> crate::Result<Self> {
        if policy == AudioPolicy::None {
            return Ok(Self {
                policy,
                selected: Vec::new(),
                skipped: streams
                    .iter()
                    .map(|stream| SkippedAudioStream {
                        input_index: stream.input_index,
                        codec: stream.codec.clone(),
                        reason: "disabled by --audio none".into(),
                    })
                    .collect(),
            });
        }

        let mut selected = Vec::new();
        let mut skipped = Vec::new();
        for stream in streams {
            if stream.mp4_compatible {
                selected.push(AudioStreamPlan {
                    input_index: stream.input_index,
                    codec: stream.codec.clone(),
                    language: stream.language.clone(),
                    default: stream.default,
                });
            } else {
                skipped.push(SkippedAudioStream {
                    input_index: stream.input_index,
                    codec: stream.codec.clone(),
                    reason: stream
                        .compatibility_reason
                        .clone()
                        .unwrap_or_else(|| "the MP4 muxer rejected this codec".into()),
                });
            }
        }

        if policy == AudioPolicy::Copy && !skipped.is_empty() {
            let details = skipped
                .iter()
                .map(|stream| {
                    format!(
                        "stream #{} ({}): {}",
                        stream.input_index, stream.codec, stream.reason
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            return Err(crate::Error::Media(format!(
                "--audio copy requires every audio stream to be MP4-compatible; {details}"
            )));
        }

        Ok(Self {
            policy,
            selected,
            skipped,
        })
    }
}
