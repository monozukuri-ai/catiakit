//! CFV2 directory framing and logical stream reconstruction.
//!
//! Field layout reference: cadmpeg, commit 6289a0d, docs/formats/catia.md §3
//! (CC BY 4.0). Geometry storage grammars are deliberately not inferred here.
use crate::{Diagnostic, DiagnosticCode as Code, ParseLimits, Severity, binary::ByteReader};
use serde::{Deserialize, Serialize};

const MAGIC: &[u8] = b"V5_CFV2\0";
const DIRECTORY_MAGIC: &[u8] = b"CATIA_V5 CB0001\0";
const END: &[u8] = b"CB__END\0";
type Result<T> = std::result::Result<T, Box<Diagnostic>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRange {
    pub offset: u64,
    pub length: u64,
}

impl FileRange {
    #[must_use]
    pub fn end(self) -> Option<u64> {
        self.offset.checked_add(self.length)
    }

    fn contains(self, other: Self) -> bool {
        match (self.end(), other.end()) {
            (Some(end), Some(other_end)) => self.offset <= other.offset && other_end <= end,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerLayout {
    OuterDirectory,
    NestedDirectories,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryKind {
    Outer,
    Nested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtentWordForm {
    Zero,
    Ordinal,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameForm {
    PrefixAsciiUtf16Le,
    InlineAsciiUtf16Le,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extent {
    pub file_range: FileRange,
    pub physical_offset: u64,
    pub logical_offset: u64,
    pub logical_length: u64,
    pub trailing_word: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamInfo {
    pub id: u32,
    pub directory_id: u32,
    pub descriptor_file_offset: u64,
    pub name: Option<String>,
    pub name_form: NameForm,
    pub logical_length: u64,
    pub extents: Vec<Extent>,
    /// Complete descriptor header, including uninterpreted words.
    pub raw_header_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnadmittedDescriptor {
    pub name: String,
    pub descriptor_file_offset: u64,
    pub raw_header_hex: String,
    pub reason: Code,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryInfo {
    pub id: u32,
    pub kind: DirectoryKind,
    pub container_offset: u64,
    pub depth: u32,
    pub owner_stream_ids: Vec<u32>,
    pub file_range: FileRange,
    pub extent_word_form: ExtentWordForm,
    pub stream_ids: Vec<u32>,
    pub unadmitted_descriptors: Vec<UnadmittedDescriptor>,
    /// Byte ranges whose framing/meaning has not been decoded.
    pub opaque_ranges: Vec<FileRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerInfo {
    pub layout: ContainerLayout,
    pub raw_outer_header_tail_hex: String,
    pub directories: Vec<DirectoryInfo>,
    pub streams: Vec<StreamInfo>,
}

impl Default for ContainerInfo {
    fn default() -> Self {
        Self {
            layout: ContainerLayout::Unknown,
            raw_outer_header_tail_hex: String::new(),
            directories: Vec::new(),
            streams: Vec::new(),
        }
    }
}

impl ContainerInfo {
    #[must_use]
    pub fn stream(&self, id: u32) -> Option<&StreamInfo> {
        self.streams.iter().find(|stream| stream.id == id)
    }
}

fn failure(code: Code, offset: u64) -> Box<Diagnostic> {
    Box::new(Diagnostic::new(code, Severity::Error, Some(offset)))
}

fn warning(code: Code, offset: u64) -> Diagnostic {
    Diagnostic::new(code, Severity::Warning, Some(offset))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|b| {
            [
                char::from(DIGITS[(b >> 4) as usize]),
                char::from(DIGITS[(b & 15) as usize]),
            ]
        })
        .collect()
}

fn u32_at(reader: &ByteReader<'_>, offset: u64) -> Result<u64> {
    reader
        .subreader(offset, 4)
        .and_then(|mut view| view.read_u32_be())
        .map(u64::from)
        .map_err(|e| Box::new(e.into_diagnostic()))
}

fn bytes_at<'a>(data: &'a [u8], range: FileRange, limits: &ParseLimits) -> Result<&'a [u8]> {
    ByteReader::new(data, 0, limits)
        .and_then(|view| view.subreader(range.offset, range.length))
        .and_then(|mut view| view.read_bytes(range.length))
        .map_err(|e| Box::new(e.into_diagnostic()))
}

#[derive(Default)]
struct Budget {
    steps: u64,
    extents: u64,
    logical_bytes: u64,
}

impl Budget {
    fn step(&mut self, limits: &ParseLimits, offset: u64) -> Result<()> {
        self.steps = self
            .steps
            .checked_add(1)
            .ok_or_else(|| failure(Code::ScanLimitExceeded, offset))?;
        if self.steps > limits.max_scan_steps {
            return Err(failure(Code::ScanLimitExceeded, offset));
        }
        Ok(())
    }
}

struct DecodedName {
    text: String,
    form: NameForm,
    start: usize,
}

fn name_at(
    data: &[u8],
    ds: usize,
    file_offset: u64,
    limits: &ParseLimits,
    budget: &mut Budget,
) -> Result<Option<DecodedName>> {
    if ds >= 19 && data.get(ds - 3..ds) == Some(&[0, 0, 0]) {
        let mut start = ds - 3;
        while start >= 18 && (0x20..0x7f).contains(&data[start - 2]) && data[start - 1] == 0 {
            budget.step(limits, file_offset + start as u64)?;
            start -= 2;
            if (ds - 3 - start) as u64 > limits.max_name_bytes {
                return Err(failure(Code::NameLimitExceeded, file_offset + start as u64));
            }
        }
        let length = ds - 3 - start;
        // A non-ASCII code unit before an ASCII suffix must not become a shortened name.
        let prefix = if start >= 2 {
            u16::from_le_bytes([data[start - 2], data[start - 1]]) as usize
        } else {
            0
        };
        if length >= 6 && (prefix < 0x20 || prefix == length + 2) {
            return Ok(Some(DecodedName {
                text: data[start..ds - 3]
                    .chunks_exact(2)
                    .map(|p| char::from(p[0]))
                    .collect(),
                form: NameForm::PrefixAsciiUtf16Le,
                start,
            }));
        }
    }
    if let Some(name_area) = data.get(ds + 16..ds + 80) {
        let count = name_area
            .chunks_exact(2)
            .take_while(|p| (0x20..0x7f).contains(&p[0]) && p[1] == 0)
            .count()
            * 2;
        if count >= 6
            && name_area.get(count..count + 2) == Some(&[0, 0])
            && name_area[count..].iter().all(|b| *b == 0)
        {
            if count as u64 > limits.max_name_bytes {
                return Err(failure(
                    Code::NameLimitExceeded,
                    file_offset + ds as u64 + 16,
                ));
            }
            return Ok(Some(DecodedName {
                text: name_area[..count]
                    .chunks_exact(2)
                    .map(|p| char::from(p[0]))
                    .collect(),
                form: NameForm::InlineAsciiUtf16Le,
                start: ds,
            }));
        }
    }
    Ok(None)
}

fn validate_extent_list(extents: &[Extent], length: u64, body: FileRange) -> Result<()> {
    let mut logical = 0u64;
    for extent in extents {
        if extent.file_range.length == 0 || !body.contains(extent.file_range) {
            return Err(failure(Code::ExtentRangeInvalid, extent.file_range.offset));
        }
        if extent.logical_offset != logical || extent.logical_length != extent.file_range.length {
            return Err(failure(
                Code::ExtentLogicalMismatch,
                extent.file_range.offset,
            ));
        }
        logical = logical
            .checked_add(extent.logical_length)
            .ok_or_else(|| failure(Code::OffsetOverflow, extent.file_range.offset))?;
    }
    if extents.is_empty() || logical != length {
        return Err(failure(Code::ExtentLogicalMismatch, body.offset));
    }
    let mut ranges: Vec<_> = extents.iter().map(|e| e.file_range).collect();
    ranges.sort_by_key(|r| r.offset);
    for pair in ranges.windows(2) {
        if pair[0].end().is_none_or(|end| end > pair[1].offset) {
            return Err(failure(Code::ExtentsOverlap, pair[1].offset));
        }
    }
    Ok(())
}

/// The sentinel normally ends the directory. Some older documents declare a
/// directory length that continues with zero bytes after the sentinel; that
/// padding is retained as an opaque range and is never scanned for descriptors.
/// Any nonzero byte after the sentinel keeps the directory invalid.
fn sentinel_position(directory: &[u8]) -> Option<usize> {
    if directory.len() < DIRECTORY_MAGIC.len() + END.len() {
        return None;
    }
    let at_end = directory.len() - END.len();
    if &directory[at_end..] == END {
        return Some(at_end);
    }
    // END finishes with a NUL, so the last nonzero byte is its final letter.
    let last_nonzero = directory.iter().rposition(|b| *b != 0)?;
    let stop = (last_nonzero + 2).checked_sub(END.len())?;
    (stop >= DIRECTORY_MAGIC.len() && directory.get(stop..stop + END.len()) == Some(END))
        .then_some(stop)
}

struct DirectoryRequest {
    base: u64,
    range: FileRange,
    kind: DirectoryKind,
    depth: u32,
    owner: Option<u32>,
}

fn parse_directory(
    data: &[u8],
    request: DirectoryRequest,
    limits: &ParseLimits,
    budget: &mut Budget,
    info: &mut ContainerInfo,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let DirectoryRequest {
        base,
        range,
        kind,
        depth,
        owner,
    } = request;
    if range.length > limits.max_directory_bytes {
        return Err(failure(Code::DirectoryLimitExceeded, range.offset));
    }
    let directory = bytes_at(data, range, limits)?;
    if !directory.starts_with(DIRECTORY_MAGIC) {
        return Err(failure(Code::DirectorySignatureInvalid, range.offset));
    }
    let stop = sentinel_position(directory)
        .ok_or_else(|| failure(Code::DirectorySentinelInvalid, range.offset))?;
    let padding = directory.len() - stop - END.len();
    if padding > 0 {
        diagnostics.push(warning(
            Code::DirectoryPaddingRetained,
            range.offset + (stop + END.len()) as u64,
        ));
    }
    let reader = ByteReader::new(directory, range.offset, limits)
        .map_err(|e| Box::new(e.into_diagnostic()))?;
    let header_length = if kind == DirectoryKind::Outer { 64 } else { 16 };
    let body_start = base
        .checked_add(header_length)
        .ok_or_else(|| failure(Code::OffsetOverflow, base))?;
    let body_length = range
        .offset
        .checked_sub(body_start)
        .ok_or_else(|| failure(Code::DirectoryRangeInvalid, range.offset))?;
    let body = FileRange {
        offset: body_start,
        length: body_length,
    };
    let directory_id = u32::try_from(info.directories.len())
        .map_err(|_| failure(Code::StreamCountExceeded, range.offset))?;
    let mut streams = Vec::new();
    let mut unadmitted = Vec::new();
    let mut occupied = vec![FileRange {
        offset: range.offset,
        length: 16,
    }];
    let mut ds = 16;
    while ds < stop {
        budget.step(limits, range.offset + ds as u64)?;
        let name = name_at(directory, ds, range.offset, limits, budget)?;
        let strong = name
            .as_ref()
            .is_some_and(|n| n.form == NameForm::PrefixAsciiUtf16Le);
        if ds + 84 > stop {
            if strong {
                return Err(failure(Code::DescriptorInvalid, range.offset + ds as u64));
            }
            ds += 1;
            continue;
        }
        let length = u32_at(&reader, ds as u64 + 12)?;
        let count = u32_at(&reader, ds as u64 + 80)?;
        if length == 0 && count == 0 && strong {
            let name = name
                .as_ref()
                .ok_or_else(|| failure(Code::DescriptorInvalid, range.offset + ds as u64))?;
            if unadmitted.len() as u64 >= limits.max_streams {
                return Err(failure(Code::StreamCountExceeded, range.offset + ds as u64));
            }
            unadmitted.push(UnadmittedDescriptor {
                name: name.text.clone(),
                descriptor_file_offset: range.offset + ds as u64,
                raw_header_hex: hex(&directory[ds..ds + 84]),
                reason: Code::EmptyDeclarationUnverified,
            });
            diagnostics.push(warning(
                Code::EmptyDeclarationUnverified,
                range.offset + ds as u64,
            ));
            occupied.push(FileRange {
                offset: range.offset + name.start as u64,
                length: (ds + 84 - name.start) as u64,
            });
            ds += 84;
            continue;
        }
        if count == 0 || length == 0 {
            if strong {
                return Err(failure(Code::DescriptorInvalid, range.offset + ds as u64));
            }
            ds += 1;
            continue;
        }
        let extent_bytes = count
            .checked_mul(20)
            .ok_or_else(|| failure(Code::OffsetOverflow, range.offset + ds as u64))?;
        let end = (ds as u64)
            .checked_add(84)
            .and_then(|v| v.checked_add(extent_bytes));
        if end.is_none_or(|end| end > stop as u64) {
            if strong {
                return Err(failure(
                    Code::DescriptorInvalid,
                    range.offset + ds as u64 + 80,
                ));
            }
            ds += 1;
            continue;
        }
        if count > limits.max_extents.saturating_sub(budget.extents) {
            if name.is_some() {
                return Err(failure(
                    Code::ExtentCountExceeded,
                    range.offset + ds as u64 + 80,
                ));
            }
            ds += 1;
            continue;
        }
        let mut extents = Vec::new();
        extents
            .try_reserve(count as usize)
            .map_err(|_| failure(Code::AllocationFailed, range.offset + ds as u64))?;
        for index in 0..count {
            let at = ds as u64 + 84 + index * 20;
            budget.step(limits, range.offset + at)?;
            let physical = u32_at(&reader, at)?;
            let size = u32_at(&reader, at + 4)?;
            let file_offset = base
                .checked_add(physical)
                .ok_or_else(|| failure(Code::OffsetOverflow, range.offset + at))?;
            extents.push(Extent {
                file_range: FileRange {
                    offset: file_offset,
                    length: size,
                },
                physical_offset: physical,
                logical_length: u32_at(&reader, at + 8)?,
                logical_offset: u32_at(&reader, at + 12)?,
                trailing_word: u32_at(&reader, at + 16)? as u32,
            });
        }
        if let Err(mut error) = validate_extent_list(&extents, length, body) {
            if strong
                || name
                    .as_ref()
                    .is_some_and(|n| n.form == NameForm::InlineAsciiUtf16Le)
            {
                error.location = Some(crate::SourceLocation::File {
                    offset: range.offset + ds as u64,
                });
                return Err(error);
            }
            ds += 1;
            continue;
        }
        if info.streams.len() as u64 + streams.len() as u64 >= limits.max_streams {
            return Err(failure(Code::StreamCountExceeded, range.offset + ds as u64));
        }
        if length > limits.max_stream_bytes {
            return Err(failure(
                Code::StreamLimitExceeded,
                range.offset + ds as u64 + 12,
            ));
        }
        budget.logical_bytes = budget
            .logical_bytes
            .checked_add(length)
            .ok_or_else(|| failure(Code::TotalStreamLimitExceeded, range.offset + ds as u64))?;
        if budget.logical_bytes > limits.max_total_stream_bytes {
            return Err(failure(
                Code::TotalStreamLimitExceeded,
                range.offset + ds as u64,
            ));
        }
        budget.extents += count;
        let id = u32::try_from(info.streams.len() + streams.len())
            .map_err(|_| failure(Code::StreamCountExceeded, range.offset + ds as u64))?;
        let name_start = name.as_ref().map_or(ds, |n| n.start);
        let end =
            end.ok_or_else(|| failure(Code::OffsetOverflow, range.offset + ds as u64))? as usize;
        if name.is_none() {
            diagnostics.push(warning(Code::NameGrammarUnknown, range.offset + ds as u64));
        }
        streams.push(StreamInfo {
            id,
            directory_id,
            descriptor_file_offset: range.offset + ds as u64,
            name_form: name.as_ref().map_or(NameForm::Unknown, |n| n.form),
            name: name.map(|n| n.text),
            logical_length: length,
            extents,
            raw_header_hex: hex(&directory[ds..ds + 84]),
        });
        occupied.push(FileRange {
            offset: range.offset + name_start as u64,
            length: (end - name_start) as u64,
        });
        ds = end;
    }
    let zero = streams
        .iter()
        .flat_map(|s| &s.extents)
        .all(|e| e.trailing_word == 0);
    let ordinal = streams.iter().all(|s| {
        s.extents
            .iter()
            .enumerate()
            .all(|(i, e)| u64::from(e.trailing_word) == i as u64 + 1)
    });
    let form = if zero {
        ExtentWordForm::Zero
    } else if ordinal {
        ExtentWordForm::Ordinal
    } else {
        ExtentWordForm::Unknown
    };
    if form == ExtentWordForm::Unknown {
        diagnostics.push(warning(Code::ExtentWordUnknown, range.offset));
    }
    if streams.is_empty() {
        diagnostics.push(warning(Code::DirectoryGrammarUnknown, range.offset));
    }
    occupied.sort_by_key(|r| r.offset);
    let mut cursor = range.offset;
    let mut opaque = Vec::new();
    for r in occupied {
        if cursor < r.offset {
            opaque.push(FileRange {
                offset: cursor,
                length: r.offset - cursor,
            });
        }
        cursor = cursor.max(
            r.end()
                .ok_or_else(|| failure(Code::OffsetOverflow, r.offset))?,
        );
    }
    let sentinel = range.offset + stop as u64;
    if cursor < sentinel {
        opaque.push(FileRange {
            offset: cursor,
            length: sentinel - cursor,
        });
    }
    if padding > 0 {
        opaque.push(FileRange {
            offset: sentinel + END.len() as u64,
            length: padding as u64,
        });
    }
    let stream_ids = streams.iter().map(|s| s.id).collect();
    info.streams.extend(streams);
    info.directories.push(DirectoryInfo {
        id: directory_id,
        kind,
        container_offset: base,
        depth,
        owner_stream_ids: owner.into_iter().collect(),
        file_range: range,
        extent_word_form: form,
        stream_ids,
        unadmitted_descriptors: unadmitted,
        opaque_ranges: opaque,
    });
    Ok(())
}

pub(crate) fn parse(
    data: &[u8],
    limits: &ParseLimits,
    info: &mut ContainerInfo,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let reader = ByteReader::new(data, 0, limits).map_err(|e| Box::new(e.into_diagnostic()))?;
    let offset = u32_at(&reader, 8)?;
    let length = u32_at(&reader, 12)?;
    if offset < 64 || offset.checked_add(length) != Some(data.len() as u64) {
        return Err(failure(Code::DirectoryRangeInvalid, 8));
    }
    info.raw_outer_header_tail_hex = hex(bytes_at(
        data,
        FileRange {
            offset: 16,
            length: 48,
        },
        limits,
    )?);
    let mut budget = Budget::default();
    parse_directory(
        data,
        DirectoryRequest {
            base: 0,
            range: FileRange { offset, length },
            kind: DirectoryKind::Outer,
            depth: 0,
            owner: None,
        },
        limits,
        &mut budget,
        info,
        diagnostics,
    )?;
    info.layout = ContainerLayout::OuterDirectory;
    let mut index = 0;
    while index < info.streams.len() {
        let stream = &info.streams[index];
        let parent = &info.directories[stream.directory_id as usize];
        let first = &stream.extents[0];
        let position = first.file_range.offset;
        if stream.logical_length < 8 {
            index += 1;
            continue;
        }
        let mut prefix = [0u8; 8];
        let mut filled = 0;
        for extent in &stream.extents {
            let count = extent.file_range.length.min((8 - filled) as u64);
            prefix[filled..filled + count as usize].copy_from_slice(bytes_at(
                data,
                FileRange {
                    offset: extent.file_range.offset,
                    length: count,
                },
                limits,
            )?);
            filled += count as usize;
            if filled == 8 {
                break;
            }
        }
        if prefix != MAGIC {
            index += 1;
            continue;
        }
        if first.file_range.length < 16 {
            diagnostics.push(warning(Code::NestedStorageUnsupported, position));
            index += 1;
            continue;
        }
        let depth = parent
            .depth
            .checked_add(1)
            .ok_or_else(|| failure(Code::NestingLimitExceeded, position))?;
        if depth > limits.max_nesting_depth {
            return Err(failure(Code::NestingLimitExceeded, position));
        }
        if let Some(existing) = info
            .directories
            .iter_mut()
            .find(|d| d.container_offset == position)
        {
            existing.owner_stream_ids.push(stream.id);
            index += 1;
            continue;
        }
        let header = reader
            .subreader(position, 16)
            .map_err(|e| Box::new(e.into_diagnostic()))?;
        let delta = u32_at(&header, 8)?;
        let size = u32_at(&header, 12)?;
        let directory = position
            .checked_add(delta)
            .ok_or_else(|| failure(Code::OffsetOverflow, position))?;
        let end = directory
            .checked_add(size)
            .ok_or_else(|| failure(Code::OffsetOverflow, position))?;
        if delta < 16
            || !first.file_range.contains(FileRange {
                offset: position,
                length: end - position,
            })
        {
            diagnostics.push(warning(Code::NestedStorageUnsupported, position));
            index += 1;
            continue;
        }
        let owner = stream.id;
        parse_directory(
            data,
            DirectoryRequest {
                base: position,
                range: FileRange {
                    offset: directory,
                    length: size,
                },
                kind: DirectoryKind::Nested,
                depth,
                owner: Some(owner),
            },
            limits,
            &mut budget,
            info,
            diagnostics,
        )?;
        info.layout = ContainerLayout::NestedDirectories;
        index += 1;
    }
    if info.streams.is_empty() {
        info.layout = ContainerLayout::Unknown;
    }
    diagnostics.push(Diagnostic::new(
        Code::GeometryLayoutUnclassified,
        Severity::Info,
        None,
    ));
    Ok(())
}

/// Reconstruct one admitted stream. Revalidates public/deserializeable metadata.
/// The caller supplies the same source bytes used for inspection.
pub fn reconstruct_stream(
    data: &[u8],
    stream: &StreamInfo,
    limits: &ParseLimits,
) -> Result<Vec<u8>> {
    if stream.logical_length > limits.max_stream_bytes {
        return Err(failure(
            Code::StreamLimitExceeded,
            stream.descriptor_file_offset,
        ));
    }
    if stream.logical_length > limits.max_total_stream_bytes {
        return Err(failure(
            Code::TotalStreamLimitExceeded,
            stream.descriptor_file_offset,
        ));
    }
    if stream.extents.len() as u64 > limits.max_extents {
        return Err(failure(
            Code::ExtentCountExceeded,
            stream.descriptor_file_offset,
        ));
    }
    validate_extent_list(
        &stream.extents,
        stream.logical_length,
        FileRange {
            offset: 0,
            length: data.len() as u64,
        },
    )?;
    let length = usize::try_from(stream.logical_length)
        .map_err(|_| failure(Code::OffsetOverflow, stream.descriptor_file_offset))?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(length)
        .map_err(|_| failure(Code::AllocationFailed, stream.descriptor_file_offset))?;
    for extent in &stream.extents {
        output.extend_from_slice(bytes_at(data, extent.file_range, limits)?);
    }
    Ok(output)
}
