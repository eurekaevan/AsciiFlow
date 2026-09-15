#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioPolicy {
    Auto,
    Copy,
    None,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(index: usize, compatible: bool, language: &str) -> AudioStreamInfo {
        AudioStreamInfo {
            input_index: index,
            codec_id: 0,
            codec: if compatible { "aac" } else { "unsupported" }.into(),
            profile: None,
            time_base: crate::Rational::new(1, 48000).unwrap(),
            sample_rate: Some(48000),
            channels: Some(1),
            bit_rate: None,
            language: Some(language.into()),
            start_time: Some(0),
            default: index == 1,
            forced: false,
            mp4_compatible: compatible,
            compatibility_reason: (!compatible).then(|| "container incompatible".into()),
        }
    }

    #[test]
    fn audio_copy_preserves_relative_order_language_and_default() {
        let input = [stream(1, true, "jpn"), stream(4, true, "eng")];
        let plan = AudioPlan::select(AudioPolicy::Copy, &input).unwrap();
        assert_eq!(
            plan.selected
                .iter()
                .map(|s| s.input_index)
                .collect::<Vec<_>>(),
            [1, 4]
        );
        assert_eq!(plan.selected[0].language.as_deref(), Some("jpn"));
        assert_eq!(plan.selected[1].language.as_deref(), Some("eng"));
        assert!(plan.selected[0].default);
        assert!(!plan.selected[1].default);
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn audio_auto_selects_only_compatible_tracks_with_skip_identity() {
        let plan = AudioPlan::select(
            AudioPolicy::Auto,
            &[stream(1, true, "jpn"), stream(3, false, "eng")],
        )
        .unwrap();
        assert_eq!(plan.selected.len(), 1);
        assert_eq!(plan.selected[0].input_index, 1);
        assert_eq!(plan.skipped.len(), 1);
        assert_eq!(plan.skipped[0].input_index, 3);
        assert_eq!(plan.skipped[0].codec, "unsupported");
        assert_eq!(plan.skipped[0].reason, "container incompatible");
    }

    #[test]
    fn audio_copy_rejects_any_incompatible_track() {
        let error = AudioPlan::select(
            AudioPolicy::Copy,
            &[stream(1, true, "jpn"), stream(3, false, "eng")],
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("stream #3"));
        assert!(message.contains("container incompatible"));
    }

    #[test]
    fn audio_none_disables_even_compatible_tracks() {
        let plan = AudioPlan::select(AudioPolicy::None, &[stream(1, true, "jpn")]).unwrap();
        assert!(plan.selected.is_empty());
        assert_eq!(plan.skipped.len(), 1);
        assert!(plan.skipped[0].reason.contains("--audio none"));
    }

    #[test]
    fn audio_without_input_tracks_succeeds_for_every_policy() {
        for policy in [AudioPolicy::Auto, AudioPolicy::Copy, AudioPolicy::None] {
            let plan = AudioPlan::select(policy, &[]).unwrap();
            assert!(plan.selected.is_empty());
            assert!(plan.skipped.is_empty());
        }
    }

    #[test]
    fn audio_auto_and_copy_have_identical_compatible_selection() {
        let input = [stream(1, true, "jpn"), stream(4, true, "eng")];
        assert_eq!(
            AudioPlan::select(AudioPolicy::Auto, &input)
                .unwrap()
                .selected,
            AudioPlan::select(AudioPolicy::Copy, &input)
                .unwrap()
                .selected
        );
    }
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
