//! Opt-in qualification instrumentation. Absent from normal builds.
use super::{ffi, packet::Packet};
use asciiflow_core::{Error, Result};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufWriter, Write},
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub(crate) struct MuxTrace(Arc<Mutex<State>>);

struct State {
    writer: BufWriter<File>,
    event: u64,
    local: BTreeMap<(&'static str, bool, usize), u64>,
}

impl MuxTrace {
    pub(crate) fn open() -> Result<Option<Self>> {
        let Some(path) = std::env::var_os("ASCIIFLOW_MUX_TRACE") else {
            return Ok(None);
        };
        Self::create(std::path::Path::new(&path)).map(Some)
    }

    pub(crate) fn create(path: &std::path::Path) -> Result<Self> {
        let file = File::options()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| Error::Media(format!("create qualification mux trace: {error}")))?;
        Ok(Self(Arc::new(Mutex::new(State {
            writer: BufWriter::new(file),
            event: 0,
            local: BTreeMap::new(),
        }))))
    }

    pub(crate) fn event(&self, boundary: &'static str, fields: String) -> Result<()> {
        let mut state = self.0.lock().expect("mux trace lock poisoned");
        let event = state.event;
        state.event += 1;
        writeln!(
            state.writer,
            "{{\"event\":{event},\"boundary\":\"{boundary}\",{fields}}}"
        )
        .and_then(|()| state.writer.flush())
        .map_err(|error| Error::Media(format!("write qualification mux trace: {error}")))
    }

    pub(crate) fn packet(
        &self,
        boundary: &'static str,
        packet: &mut Packet,
        stream: usize,
        audio: bool,
        tb: ffi::AVRational,
    ) -> Result<()> {
        let fields = Self::snapshot(packet, stream, audio, tb)?;
        self.accepted(boundary, stream, audio, fields)
    }

    pub(crate) fn accepted(
        &self,
        boundary: &'static str,
        stream: usize,
        audio: bool,
        fields: String,
    ) -> Result<()> {
        let local = {
            let mut state = self.0.lock().expect("mux trace lock poisoned");
            let seq = state.local.entry((boundary, audio, stream)).or_default();
            let value = *seq;
            *seq += 1;
            value
        };
        self.event(boundary, format!("\"local_seq\":{local},{fields}"))
    }

    pub(crate) fn snapshot(
        packet: &mut Packet,
        stream: usize,
        audio: bool,
        tb: ffi::AVRational,
    ) -> Result<String> {
        let p = unsafe { &*packet.as_mut_ptr() };
        let hash = payload_hash(p)?;
        Ok(format!(
            "\"stream\":{stream},\"audio\":{audio},\"pts\":{},\"dts\":{},\"duration\":{},\"tb\":[{},{}],\"flags\":{},\"size\":{},\"payload_sha256\":\"{hash}\"",
            p.pts, p.dts, p.duration, tb.num, tb.den, p.flags, p.size
        ))
    }
}

fn payload_hash(packet: &ffi::AVPacket) -> Result<String> {
    let sha = unsafe { ffi::av_sha_alloc() };
    if sha.is_null() {
        return Err(Error::Media("allocate mux trace SHA-256".into()));
    }
    let mut bytes = [0_u8; 32];
    unsafe {
        ffi::av_sha_init(sha, 256);
        ffi::av_sha_update(sha, packet.data, packet.size.max(0) as usize);
        ffi::av_sha_final(sha, bytes.as_mut_ptr());
        ffi::av_free(sha.cast());
    }
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
