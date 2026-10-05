//! A bounded merge of two independently advancing producers.
//!
//! Audio retains the input demuxer's order across all selected tracks. This is
//! not a global per-track sort: libavformat still owns container interleaving.
//! Only one head per producer is retained, and wall-clock arrival never selects
//! the next write. The legacy single FIFO exists only for qualification replay.
use super::*;

pub(super) struct MuxInputs<'a> {
    pub(super) video: &'a Receiver<MuxMessage>,
    pub(super) audio: Option<&'a Receiver<MuxMessage>>,
}

pub(super) struct MuxInbox<'a> {
    video: &'a Receiver<MuxMessage>,
    audio: Option<&'a Receiver<MuxMessage>>,
    output: &'a MuxOutput,
    video_head: Option<MuxMessage>,
    audio_head: Option<MuxMessage>,
    video_done: bool,
    audio_done: bool,
    #[cfg(feature = "mux-qualification")]
    trace: Option<super::super::mux_trace::MuxTrace>,
}

impl<'a> MuxInbox<'a> {
    pub(super) fn new(
        video: &'a Receiver<MuxMessage>,
        audio: Option<&'a Receiver<MuxMessage>>,
        output: &'a MuxOutput,
    ) -> Self {
        Self {
            video,
            audio,
            output,
            video_head: None,
            audio_head: None,
            video_done: false,
            audio_done: output.audio_routes.is_empty(),
            #[cfg(feature = "mux-qualification")]
            trace: None,
        }
    }

    pub(super) fn receive(&mut self, video_offset: i64) -> Result<Option<MuxMessage>> {
        let Some(audio) = self.audio else {
            return self.poll(self.video);
        };
        if self.video_head.is_none() && !self.video_done {
            let Some(message) = self.poll(self.video)? else {
                return Ok(None);
            };
            match message {
                MuxMessage::Finish => self.video_done = true,
                packet @ MuxMessage::Packet { .. } => self.video_head = Some(packet),
                control => return Ok(Some(control)),
            }
        }
        if self.audio_head.is_none() && !self.audio_done {
            let Some(message) = self.poll(audio)? else {
                return Ok(None);
            };
            match message {
                MuxMessage::AudioDone => {
                    self.audio_done = true;
                    return Ok(Some(MuxMessage::AudioDone));
                }
                packet @ MuxMessage::Packet { .. } => self.audio_head = Some(packet),
                control => return Ok(Some(control)),
            }
        }
        let take_video = match (&mut self.video_head, &mut self.audio_head) {
            (Some(video), Some(audio)) => {
                Self::output_order(self.output, video, audio, video_offset)?
            }
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => return Ok(Some(MuxMessage::Finish)),
        };
        Ok(Some(
            if take_video {
                self.video_head.take()
            } else {
                self.audio_head.take()
            }
            .expect("selected producer has a head"),
        ))
    }

    #[cfg(feature = "mux-qualification")]
    pub(super) fn set_trace(&mut self, trace: Option<super::super::mux_trace::MuxTrace>) {
        self.trace = trace;
    }

    fn poll(&self, receiver: &Receiver<MuxMessage>) -> Result<Option<MuxMessage>> {
        match receiver.recv_timeout(MUX_POLL) {
            Ok(mut message) => {
                #[cfg(feature = "mux-qualification")]
                if let Some(trace) = &self.trace
                    && let MuxMessage::Packet {
                        packet,
                        input_index,
                        input_time_base,
                        audio,
                    } = &mut message
                {
                    trace.packet("C-receive", packet, *input_index, *audio, *input_time_base)?;
                }
                #[cfg(not(feature = "mux-qualification"))]
                let _ = &mut message;
                Ok(Some(message))
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => Ok(None),
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => Err(Error::pipeline_message(
                PipelineStage::MuxRuntime,
                "receive mux packet",
                "mux producer disconnected before explicit EOF",
            )),
        }
    }

    fn output_order(
        output: &MuxOutput,
        video: &mut MuxMessage,
        audio: &mut MuxMessage,
        offset: i64,
    ) -> Result<bool> {
        let timestamp = |message: &mut MuxMessage| {
            let MuxMessage::Packet {
                packet,
                input_index,
                input_time_base,
                audio,
            } = message
            else {
                unreachable!()
            };
            normalized_timestamps(
                output,
                packet,
                *input_index,
                *input_time_base,
                *audio,
                offset,
            )
        };
        let (vdts, vpts, vtb, vi) = timestamp(video)?;
        let (adts, apts, atb, ai) = timestamp(audio)?;
        let dts = unsafe { ffi::av_compare_ts(vdts, vtb, adts, atb) };
        let pts = unsafe { ffi::av_compare_ts(vpts, vtb, apts, atb) };
        Ok(dts < 0 || (dts == 0 && (pts < 0 || (pts == 0 && vi <= ai))))
    }
}

/// Compare and emit using exactly the same output ticks and checked offsets.
pub(super) fn normalized_timestamps(
    output: &MuxOutput,
    packet: &mut Packet,
    input_index: usize,
    input_time_base: ffi::AVRational,
    audio: bool,
    offset: i64,
) -> Result<(i64, i64, ffi::AVRational, i32)> {
    let (index, tb) = if audio {
        let route = output
            .audio_routes
            .iter()
            .find(|route| route.input_index == input_index)
            .ok_or_else(|| Error::Media("selected audio route disappeared".into()))?;
        (route.output_index, route.output_time_base)
    } else {
        (output.video_stream_index, output.video_time_base)
    };
    if input_time_base.num <= 0 || input_time_base.den <= 0 || tb.num <= 0 || tb.den <= 0 {
        return Err(Error::Media("invalid mux packet time base".into()));
    }
    let native = unsafe { &*packet.as_mut_ptr() };
    let rescale = |value| -> Result<i64> {
        if value == ffi::AV_NOPTS_VALUE {
            return Err(Error::Media("mux packet has missing PTS or DTS".into()));
        }
        let scaled = unsafe { ffi::av_rescale_q(value, input_time_base, tb) };
        if scaled == ffi::AV_NOPTS_VALUE {
            return Err(Error::Media("mux timestamp rescale overflow".into()));
        }
        scaled
            .checked_add(if audio { 0 } else { offset })
            .filter(|value| *value != ffi::AV_NOPTS_VALUE)
            .ok_or_else(|| Error::Media("video timestamp offset overflow".into()))
    };
    Ok((rescale(native.dts)?, rescale(native.pts)?, tb, index))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output() -> MuxOutput {
        MuxOutput {
            format: NonNull::new(unsafe { ffi::avformat_alloc_context() }).unwrap(),
            video_stream_index: 0,
            video_time_base: ffi::AVRational { num: 1, den: 1000 },
            audio_routes: vec![AudioMuxRoute {
                input_index: 7,
                output_index: 1,
                output_time_base: ffi::AVRational { num: 1, den: 48000 },
            }],
        }
    }
    fn packet(audio: bool, dts: i64, pts: i64) -> MuxMessage {
        let mut packet = Packet::new().unwrap();
        unsafe {
            (*packet.as_mut_ptr()).dts = dts;
            (*packet.as_mut_ptr()).pts = pts;
        }
        MuxMessage::Packet {
            packet,
            input_index: if audio { 7 } else { 0 },
            input_time_base: ffi::AVRational {
                num: 1,
                den: if audio { 48000 } else { 1000 },
            },
            audio,
        }
    }

    #[test]
    fn rational_dts_then_pts_then_output_stream_ties() {
        let output = output();
        assert!(
            MuxInbox::output_order(
                &output,
                &mut packet(false, -20, 40),
                &mut packet(true, 0, 0),
                0
            )
            .unwrap()
        );
        assert!(
            !MuxInbox::output_order(
                &output,
                &mut packet(false, 10, 20),
                &mut packet(true, 480, 480),
                0
            )
            .unwrap()
        );
        assert!(
            MuxInbox::output_order(
                &output,
                &mut packet(false, 10, 10),
                &mut packet(true, 480, 480),
                0
            )
            .unwrap()
        );
        assert!(
            !MuxInbox::output_order(
                &output,
                &mut packet(false, 0, 0),
                &mut packet(true, 480, 480),
                20
            )
            .unwrap()
        );
    }

    #[test]
    fn invalid_timestamps_fail_before_choosing_a_head() {
        let output = output();
        assert!(
            MuxInbox::output_order(
                &output,
                &mut packet(false, ffi::AV_NOPTS_VALUE, 0),
                &mut packet(true, 0, 0),
                0
            )
            .is_err()
        );
        assert!(
            MuxInbox::output_order(
                &output,
                &mut packet(false, i64::MAX, i64::MAX),
                &mut packet(true, 0, 0),
                1
            )
            .is_err()
        );
    }

    #[test]
    fn zero_video_frames_and_audio_eof_drain_before_finish() {
        let output = output();
        let (vtx, vrx) = bounded(2);
        let (atx, arx) = bounded(2);
        vtx.send(MuxMessage::Finish).unwrap();
        atx.send(packet(true, -1024, -1024)).unwrap();
        atx.send(MuxMessage::AudioDone).unwrap();
        let mut inbox = MuxInbox::new(&vrx, Some(&arx), &output);
        assert!(matches!(
            inbox.receive(0).unwrap(),
            Some(MuxMessage::Packet { audio: true, .. })
        ));
        assert!(matches!(
            inbox.receive(0).unwrap(),
            Some(MuxMessage::AudioDone)
        ));
        assert!(matches!(
            inbox.receive(0).unwrap(),
            Some(MuxMessage::Finish)
        ));
    }

    #[test]
    fn waiting_for_audio_retains_video_head_and_detects_disconnect() {
        let output = output();
        let (vtx, vrx) = bounded(2);
        let (atx, arx) = bounded(2);
        vtx.send(packet(false, 0, 0)).unwrap();
        let mut inbox = MuxInbox::new(&vrx, Some(&arx), &output);
        assert!(inbox.receive(0).unwrap().is_none());
        assert!(inbox.video_head.is_some());
        drop(atx);
        assert!(
            inbox
                .receive(0)
                .err()
                .unwrap()
                .to_string()
                .contains("explicit EOF")
        );
    }
}
