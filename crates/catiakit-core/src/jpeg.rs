//! Bounded JPEG marker framing for previews (ITU-T T.81, Annex B).
//! This does not decode entropy-coded coefficients or certify pixel validity.
use crate::{DiagnosticCode as Code, ParseLimits};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JpegInfo {
    pub width: u32,
    pub height: u32,
    pub components: u8,
    pub sof_marker: u8,
    pub byte_length: u64,
    pub(crate) work_steps: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JpegError {
    pub code: Code,
    pub offset: usize,
}

fn error(code: Code, offset: usize) -> JpegError {
    JpegError { code, offset }
}

/// Accept one exactly bounded 8-bit baseline/progressive Huffman JPEG frame.
pub fn inspect_jpeg(data: &[u8], limits: &ParseLimits) -> Result<JpegInfo, JpegError> {
    let info = inspect_jpeg_prefix(data, limits)?;
    if info.byte_length != data.len() as u64 {
        return Err(error(Code::PreviewInvalid, info.byte_length as usize));
    }
    Ok(info)
}

/// Validate the first framed JPEG, returning its exact EOI boundary. Remaining
/// bytes are not interpreted as part of the image or as additional previews.
pub fn inspect_jpeg_prefix(data: &[u8], limits: &ParseLimits) -> Result<JpegInfo, JpegError> {
    if data.len() as u64 > limits.max_preview_bytes {
        return Err(error(Code::PreviewLimitExceeded, 0));
    }
    if !data.starts_with(&[0xff, 0xd8]) {
        return Err(error(Code::PreviewInvalid, 0));
    }
    let mut pos = 2usize;
    let mut steps = 0u64;
    let mut frame = None;
    let mut component_ids = [false; 256];
    let mut scans = 0;
    let mut in_scan = false;
    let mut entropy_bytes = 0;
    let mut restart_interval = 0;
    let mut next_restart = 0;
    loop {
        steps += 1;
        if steps > limits.max_metadata_steps {
            return Err(error(Code::MetadataStepLimitExceeded, pos));
        }
        let invalid = || error(Code::PreviewInvalid, pos);
        if in_scan && data.get(pos).is_some_and(|b| *b != 0xff) {
            entropy_bytes += 1;
            pos += 1;
            continue;
        }
        if data.get(pos) != Some(&0xff) {
            return Err(invalid());
        }
        let marker_start = pos;
        while data.get(pos) == Some(&0xff) {
            pos += 1;
            steps += 1;
            if steps > limits.max_metadata_steps {
                return Err(error(Code::MetadataStepLimitExceeded, pos));
            }
        }
        let marker = *data
            .get(pos)
            .ok_or_else(|| error(Code::PreviewInvalid, pos))?;
        pos += 1;
        if in_scan {
            if marker == 0 {
                entropy_bytes += 1;
                continue;
            }
            if (0xd0..=0xd7).contains(&marker) {
                if restart_interval == 0 || marker != 0xd0 + next_restart {
                    return Err(error(Code::PreviewInvalid, marker_start));
                }
                next_restart = (next_restart + 1) % 8;
                continue;
            }
            if entropy_bytes == 0 {
                return Err(error(Code::PreviewInvalid, marker_start));
            }
            in_scan = false;
        }
        if marker == 0xd9 {
            if scans == 0 {
                return Err(error(Code::PreviewInvalid, marker_start));
            }
            let mut info: JpegInfo =
                frame.ok_or_else(|| error(Code::PreviewInvalid, marker_start))?;
            info.work_steps = steps;
            info.byte_length = pos as u64;
            return Ok(info);
        }
        if matches!(marker, 0 | 0xd8 | 0xd0..=0xd7) {
            return Err(error(Code::PreviewInvalid, marker_start));
        }
        let length_bytes = data
            .get(pos..pos + 2)
            .ok_or_else(|| error(Code::PreviewInvalid, pos))?;
        let length = u16::from_be_bytes([length_bytes[0], length_bytes[1]]) as usize;
        if length < 2 {
            return Err(error(Code::PreviewInvalid, pos));
        }
        let payload = data
            .get(pos + 2..pos + length)
            .ok_or_else(|| error(Code::PreviewInvalid, pos))?;
        steps += length as u64;
        if steps > limits.max_metadata_steps {
            return Err(error(Code::MetadataStepLimitExceeded, pos));
        }
        pos += length;
        match marker {
            0xc0 | 0xc2 => {
                if frame.is_some() || payload.len() < 6 {
                    return Err(error(Code::PreviewInvalid, marker_start));
                }
                if payload[0] != 8 {
                    return Err(error(Code::PreviewUnsupported, marker_start));
                }
                let height = u16::from_be_bytes([payload[1], payload[2]]) as u32;
                let width = u16::from_be_bytes([payload[3], payload[4]]) as u32;
                let components = payload[5];
                if height == 0
                    || width == 0
                    || !(1..=4).contains(&components)
                    || payload.len() != 6 + 3 * components as usize
                {
                    return Err(error(Code::PreviewInvalid, marker_start));
                }
                if u64::from(width) * u64::from(height) > limits.max_preview_pixels {
                    return Err(error(Code::PreviewPixelLimitExceeded, marker_start));
                }
                for c in payload[6..].chunks_exact(3) {
                    if component_ids[c[0] as usize]
                        || !(1..=4).contains(&(c[1] >> 4))
                        || !(1..=4).contains(&(c[1] & 15))
                        || c[2] > 3
                    {
                        return Err(error(Code::PreviewInvalid, marker_start));
                    }
                    component_ids[c[0] as usize] = true;
                }
                frame = Some(JpegInfo {
                    width,
                    height,
                    components,
                    sof_marker: marker,
                    byte_length: 0,
                    work_steps: 0,
                });
            }
            0xda => {
                let info = frame.ok_or_else(|| error(Code::PreviewInvalid, marker_start))?;
                let count = usize::from(
                    *payload
                        .first()
                        .ok_or_else(|| error(Code::PreviewInvalid, marker_start))?,
                );
                if count == 0 || count > info.components as usize || payload.len() != 4 + 2 * count
                {
                    return Err(error(Code::PreviewInvalid, marker_start));
                }
                let mut used = [false; 256];
                for c in payload[1..1 + 2 * count].chunks_exact(2) {
                    if !component_ids[c[0] as usize]
                        || used[c[0] as usize]
                        || c[1] >> 4 > 3
                        || c[1] & 15 > 3
                    {
                        return Err(error(Code::PreviewInvalid, marker_start));
                    }
                    used[c[0] as usize] = true;
                }
                let tail = &payload[1 + 2 * count..];
                if (info.sof_marker == 0xc0 && tail != [0, 63, 0])
                    || tail[0] > tail[1]
                    || tail[1] > 63
                    || tail[2] >> 4 > 13
                    || tail[2] & 15 > 13
                {
                    return Err(error(Code::PreviewInvalid, marker_start));
                }
                scans += 1;
                in_scan = true;
                entropy_bytes = 0;
                next_restart = 0;
            }
            0xdd => {
                if payload.len() != 2 {
                    return Err(error(Code::PreviewInvalid, marker_start));
                }
                restart_interval = u16::from_be_bytes([payload[0], payload[1]]);
            }
            0xdb => {
                let mut i = 0;
                while i < payload.len() {
                    let header = payload[i];
                    if header >> 4 > 1 || header & 15 > 3 {
                        return Err(error(Code::PreviewInvalid, marker_start));
                    }
                    i += 1 + 64 * (1 + (header >> 4) as usize);
                }
                if i == 0 || i != payload.len() {
                    return Err(error(Code::PreviewInvalid, marker_start));
                }
            }
            0xc4 => {
                let mut i = 0;
                while i < payload.len() {
                    let header = payload[i];
                    let counts = payload
                        .get(i + 1..i + 17)
                        .ok_or_else(|| error(Code::PreviewInvalid, marker_start))?;
                    let total: usize = counts.iter().map(|n| *n as usize).sum();
                    if header >> 4 > 1 || header & 15 > 3 || total == 0 || total > 256 {
                        return Err(error(Code::PreviewInvalid, marker_start));
                    }
                    i += 17 + total;
                }
                if i == 0 || i != payload.len() {
                    return Err(error(Code::PreviewInvalid, marker_start));
                }
            }
            0xe0..=0xef | 0xfe => {}
            _ => return Err(error(Code::PreviewUnsupported, marker_start)),
        }
    }
}
