//! Counted reference lanes following a parameter map. The tail and allocation
//! forms remain source data; this does not assign endpoint or support roles.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeCountedReferenceFields {
    pub references: Vec<NativeAllocationReference>,
    pub opaque_tail: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeCountedReferenceFrame {
    pub frame_width: u8,
    pub header_token: Vec<u8>,
    pub fields: NativeCountedReferenceFields,
    pub source: StreamSpan,
    pub raw_frame_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeParameterMapContext {
    pub map: String,
    pub run: u32,
    pub following: [NativeCountedReferenceFrame; 2],
    pub source: StreamSpan,
}

pub(super) fn parse(bytes: &[u8]) -> Option<NativeCountedReferenceFields> {
    let mut c = Cursor::new(bytes);
    let count = c.byte()?.checked_sub(128)? as usize;
    if !(1..=127).contains(&count) || count >= bytes.len().saturating_sub(c.p) {
        return None;
    }
    let mut references = Vec::with_capacity(count);
    for _ in 0..count {
        references.push(parameter_maps::reference(&mut c)?);
    }
    let opaque_tail = bytes.get(c.p..)?.to_vec();
    (!opaque_tail.is_empty()).then_some(NativeCountedReferenceFields {
        references,
        opaque_tail,
    })
}

pub(super) fn decode(
    bytes: &[u8],
    fs: &[Frame],
    stream: &StreamInfo,
    map: &NativeParameterMap,
    b: &mut Budget<'_>,
) -> Result<Option<NativeParameterMapContext>> {
    if fs.len() != 3
        || !fs[1..]
            .iter()
            .all(|f| f.family == b'B' && f.class == 6 && f.flag == 3)
    {
        return Ok(None);
    }
    let mut following = Vec::with_capacity(2);
    for f in &fs[1..] {
        let Some(fields) = parse(&bytes[f.payload..f.end]) else {
            return Ok(None);
        };
        b.add(fields.references.len() + 1)?;
        following.push(NativeCountedReferenceFrame {
            frame_width: f.width,
            header_token: f.token_bytes.clone(),
            fields,
            source: span(stream, f.start, f.end, b)?,
            raw_frame_hex: bytes[f.start..f.end]
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect(),
        });
    }
    b.add(1)?;
    Ok(Some(NativeParameterMapContext {
        map: map.id.clone(),
        run: map.run,
        following: following.try_into().unwrap(),
        source: span(stream, fs[0].start, fs[2].end, b)?,
    }))
}
