//! Narrow AVCC H.264 packet comparison for the observed VAAPI encoder SEI.
//! Raw byte identity is reported by the caller and is never rewritten here.

const ENCODER_UUID: [u8; 16] = [
    0x59, 0x94, 0x8b, 0x28, 0x11, 0xec, 0x45, 0xaf, 0x96, 0x75, 0x19, 0xd4, 0x1f, 0xea, 0xa9, 0x4d,
];
const ENCODER_PREFIX: &[u8] = b"Lavc62.28.";
const ENCODER_SUFFIX: &[u8] =
    b" / VAAPI 1.23.0 / Intel iHD driver for Intel(R) Gen Graphics - 26.1.5 ()\0";

#[derive(Debug, Eq, PartialEq)]
pub struct ApprovedVersionChange {
    pub nal_index: usize,
    pub sei_index: usize,
    pub uuid: [u8; 16],
    pub reference_text: String,
    pub candidate_text: String,
    pub reference_token: Vec<u8>,
    pub candidate_token: Vec<u8>,
}

#[derive(Debug)]
pub struct NalUnit<'a> {
    pub kind: u8,
    pub bytes: &'a [u8],
}

pub fn avcc_length_size(extradata: &[u8]) -> Result<usize, String> {
    if extradata.len() < 5 || extradata[0] != 1 {
        return Err("H.264 extradata is not an AVCDecoderConfigurationRecord".into());
    }
    let size = usize::from(extradata[4] & 3) + 1;
    if size == 3 {
        return Err("three-byte AVCC NAL lengths are reserved".into());
    }
    Ok(size)
}

pub fn parse_avcc(packet: &[u8], length_size: usize) -> Result<Vec<NalUnit<'_>>, String> {
    if !matches!(length_size, 1 | 2 | 4) {
        return Err("unsupported AVCC NAL length size".into());
    }
    let mut cursor = 0usize;
    let mut units = Vec::new();
    while cursor < packet.len() {
        let length_end = cursor
            .checked_add(length_size)
            .filter(|&end| end <= packet.len())
            .ok_or("truncated AVCC NAL length")?;
        let length = packet[cursor..length_end]
            .iter()
            .fold(0usize, |value, &byte| (value << 8) | usize::from(byte));
        cursor = length_end;
        let end = cursor
            .checked_add(length)
            .filter(|&end| length > 0 && end <= packet.len())
            .ok_or("invalid or truncated AVCC NAL")?;
        let bytes = &packet[cursor..end];
        if bytes[0] & 0x80 != 0 || bytes[0] & 0x1f == 0 {
            return Err("invalid H.264 NAL header".into());
        }
        units.push(NalUnit {
            kind: bytes[0] & 0x1f,
            bytes,
        });
        cursor = end;
    }
    if units.is_empty() {
        return Err("AVCC packet contains no NAL".into());
    }
    Ok(units)
}

#[derive(Debug, Eq, PartialEq)]
struct SeiMessage {
    kind: usize,
    payload: Vec<u8>,
}

fn rbsp(ebsp: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(ebsp.len());
    let mut zeros = 0;
    for (index, &byte) in ebsp.iter().enumerate() {
        if zeros >= 2 && byte == 3 {
            if ebsp.get(index + 1).is_none_or(|&next| next > 3) {
                return Err("invalid H.264 emulation-prevention byte".into());
            }
            zeros = 0;
            continue;
        }
        out.push(byte);
        zeros = if byte == 0 { zeros + 1 } else { 0 };
    }
    Ok(out)
}

fn sei_number(data: &[u8], cursor: &mut usize) -> Result<usize, String> {
    let mut number = 0usize;
    loop {
        let byte = *data.get(*cursor).ok_or("truncated SEI type or size")?;
        *cursor += 1;
        number = number
            .checked_add(usize::from(byte))
            .ok_or("SEI type or size overflow")?;
        if byte != 0xff {
            return Ok(number);
        }
    }
}

fn parse_sei(nal: &[u8]) -> Result<Vec<SeiMessage>, String> {
    if nal.first().is_none_or(|header| header & 0x1f != 6) {
        return Err("not an SEI NAL".into());
    }
    let data = rbsp(&nal[1..])?;
    let mut cursor = 0;
    let mut messages = Vec::new();
    while cursor < data.len() {
        if data[cursor..] == [0x80] {
            return Ok(messages);
        }
        let kind = sei_number(&data, &mut cursor)?;
        let size = sei_number(&data, &mut cursor)?;
        let end = cursor
            .checked_add(size)
            .filter(|&end| end < data.len())
            .ok_or("SEI payload exceeds RBSP or has no trailing bits")?;
        messages.push(SeiMessage {
            kind,
            payload: data[cursor..end].to_vec(),
        });
        cursor = end;
    }
    Err("SEI RBSP lacks trailing bits".into())
}

fn encoder_version_change(
    reference: &[u8],
    candidate: &[u8],
    nal_index: usize,
    sei_index: usize,
) -> Result<ApprovedVersionChange, String> {
    if reference.len() < 16 || candidate.len() < 16 {
        return Err("user_data_unregistered lacks UUID".into());
    }
    if reference[..16] != ENCODER_UUID || candidate[..16] != ENCODER_UUID {
        return Err("user_data_unregistered UUID is not the approved encoder identifier".into());
    }
    let a = &reference[16..];
    let b = &candidate[16..];
    if a.len() != b.len()
        || !a.starts_with(ENCODER_PREFIX)
        || !b.starts_with(ENCODER_PREFIX)
        || !a.ends_with(ENCODER_SUFFIX)
        || !b.ends_with(ENCODER_SUFFIX)
    {
        return Err("encoder identifier does not match the pinned VAAPI pattern".into());
    }
    let token_end = a.len() - ENCODER_SUFFIX.len();
    let a_patch = &a[ENCODER_PREFIX.len()..token_end];
    let b_patch = &b[ENCODER_PREFIX.len()..token_end];
    if a_patch.len() != 3
        || !a_patch.iter().all(u8::is_ascii_digit)
        || !b_patch.iter().all(u8::is_ascii_digit)
        || a_patch == b_patch
    {
        return Err(
            "encoder identifier difference is not solely a three-digit patch version".into(),
        );
    }
    Ok(ApprovedVersionChange {
        nal_index,
        sei_index,
        uuid: ENCODER_UUID,
        reference_text: String::from_utf8(a[..a.len() - 1].to_vec())
            .map_err(|_| "encoder identifier is not ASCII/UTF-8")?,
        candidate_text: String::from_utf8(b[..b.len() - 1].to_vec())
            .map_err(|_| "encoder identifier is not ASCII/UTF-8")?,
        reference_token: a[..token_end].to_vec(),
        candidate_token: b[..token_end].to_vec(),
    })
}

pub fn compare_avcc_packets(
    reference: &[u8],
    candidate: &[u8],
    length_size: usize,
) -> Result<Vec<ApprovedVersionChange>, String> {
    let a = parse_avcc(reference, length_size)?;
    let b = parse_avcc(candidate, length_size)?;
    if a.len() != b.len() {
        return Err("H.264 NAL count changed".into());
    }
    let mut approved = Vec::new();
    for (nal_index, (x, y)) in a.iter().zip(&b).enumerate() {
        if x.kind != y.kind {
            return Err(format!("H.264 NAL {nal_index} type/order changed"));
        }
        if x.bytes == y.bytes {
            continue;
        }
        if x.kind != 6 || x.bytes[0] != y.bytes[0] || x.bytes.len() != y.bytes.len() {
            return Err(format!("H.264 NAL {nal_index} type {} changed", x.kind));
        }
        let x_messages = parse_sei(x.bytes)?;
        let y_messages = parse_sei(y.bytes)?;
        if x_messages.len() != y_messages.len() {
            return Err(format!("H.264 SEI NAL {nal_index} message count changed"));
        }
        let previous_count = approved.len();
        for (sei_index, (before, after)) in x_messages.iter().zip(&y_messages).enumerate() {
            if before == after {
                continue;
            }
            if before.kind != 5 || after.kind != 5 {
                return Err(format!(
                    "H.264 SEI NAL {nal_index} message {sei_index} changed"
                ));
            }
            approved.push(encoder_version_change(
                &before.payload,
                &after.payload,
                nal_index,
                sei_index,
            )?);
        }
        if approved.len() == previous_count {
            return Err(format!(
                "H.264 SEI NAL {nal_index} bytes changed outside messages"
            ));
        }
        let mut masked_before = x.bytes.to_vec();
        let mut masked_after = y.bytes.to_vec();
        for change in &approved[previous_count..] {
            masked_before = mask_unique_token(&masked_before, &change.reference_token)?;
            masked_after = mask_unique_token(&masked_after, &change.candidate_token)?;
        }
        if masked_before != masked_after {
            return Err(format!(
                "H.264 SEI NAL {nal_index} raw bytes changed outside approved patch token"
            ));
        }
    }
    if approved.len() != 1 {
        return Err("H.264 packet must differ in exactly one approved encoder SEI".into());
    }
    Ok(approved)
}

fn mask_unique_token(nal: &[u8], token: &[u8]) -> Result<Vec<u8>, String> {
    let positions: Vec<_> = nal
        .windows(token.len())
        .enumerate()
        .filter_map(|(index, bytes)| (bytes == token).then_some(index))
        .collect();
    if positions.len() != 1 {
        return Err("approved encoder token is not unique in its SEI NAL".into());
    }
    let mut masked = nal.to_vec();
    masked[positions[0]..positions[0] + token.len()].fill(0);
    Ok(masked)
}
