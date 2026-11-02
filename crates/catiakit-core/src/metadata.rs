//! Metadata in admitted outer named streams. No whole-file signature search.
//!
//! Four outer stream families are decoded: `CATPropertySets` (typed property
//! sets, including the saved-by version), `CATStorageProperty` (storage
//! declarations: the saved document name, the document type and the
//! application containers), `CATDocumentProperty` (the document link table)
//! and `CATPreview` (framed JPEG previews). Every admitted record keeps its
//! source span; bytes outside the admitted grammar stay opaque.
use crate::{
    Diagnostic, DiagnosticCode as Code, ParseLimits, Severity, SourceLocation,
    binary::ByteReader,
    container::{ContainerInfo, DirectoryKind, FileRange, StreamInfo, reconstruct_stream},
    jpeg::inspect_jpeg_prefix,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataState {
    #[default]
    NotFound,
    Present,
    Conflicting,
    Invalid,
    Unsupported,
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamSpan {
    pub stream_id: u32,
    pub logical_offset: u64,
    pub length: u64,
    /// Physical pieces in logical order, not an enclosing physical interval.
    pub file_ranges: Vec<FileRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedVersion {
    pub version: String,
    pub release: String,
    pub service_pack: String,
    pub build_date: String,
    /// Absent in the examined V5R12 files, whose tuples end with the build date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hot_fix: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionObservation {
    pub value: SavedVersion,
    pub source: StreamSpan,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionInfo {
    pub state: MetadataState,
    pub value: Option<SavedVersion>,
    pub observations: Vec<VersionObservation>,
}

/// Typed value of one property in a `CATPropertySets` stream.
///
/// Type codes are the stored `u32` discriminators. Text outside printable
/// ASCII and binary payloads are retained as hex rather than decoded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum PropertyValue {
    /// Type code 2.
    U16(u16),
    /// Type code 3.
    I32(i32),
    /// Type code 12. Every element carries its own type code.
    List(Vec<PropertyValue>),
    /// Type code 13, or type code 14 text outside printable ASCII.
    Bytes(String),
    /// Type code 14 with printable ASCII content.
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Property {
    pub name: String,
    pub type_code: u32,
    pub value: PropertyValue,
    pub source: StreamSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertySet {
    pub stream_id: u32,
    pub name: String,
    /// `false` when the set is declared by name only and carries no inline
    /// properties. `CATPreview` is declared this way in every examined file.
    pub inline: bool,
    pub properties: Vec<Property>,
    pub source: StreamSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageRole {
    /// Kind 1: owner `CATIA` and the document name recorded at save time.
    SavedName,
    /// Kind 2: the document type declared by the root storage.
    Document,
    /// Kind 3: one application container inside the document.
    Container,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageDeclaration {
    pub stream_id: u32,
    pub role: StorageRole,
    pub kind_byte: u8,
    /// First byte of the stream envelope: `M` in V5R21 and later files,
    /// `N` in the examined V5R14 to V5R19 files.
    pub envelope_byte: u8,
    /// First string value: `CATIA` for saved names, the document or container type otherwise.
    pub type_name: String,
    /// Second string value: the saved document name, or the container name.
    pub name: String,
    /// 16-byte identifiers in stored order.
    pub ids_hex: Vec<String>,
    pub source: StreamSpan,
    pub name_source: StreamSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentDeclaration {
    pub type_name: String,
    pub name: String,
    pub document_id_hex: String,
    pub stream_id: u32,
    pub source: StreamSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedName {
    /// Stored string; empty in every examined V5R21 and later file.
    pub name: String,
    pub stream_id: u32,
    pub source: StreamSpan,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageInfo {
    pub state: MetadataState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<DocumentDeclaration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_name: Option<SavedName>,
    #[serde(default)]
    pub declarations: Vec<StorageDeclaration>,
}

/// One decoded locator payload of the document link table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Locator {
    /// Locator family name as stored: `GenericLocate`, `CATIA` or `CATWin` were observed.
    pub kind: String,
    /// Leading identifier bytes; a 16-byte UUID follows one marker byte in the `CATIA` family.
    pub id_hex: String,
    pub word: u32,
    /// Stored document type such as `CATPart`, `CATProduct`, `CATMaterial` or `feat`.
    pub document_type: String,
    /// Stored path when present and valid UTF-8.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Stored path bytes when they are not valid UTF-8.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_hex: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentLink {
    pub ordinal: u32,
    /// `None` when the payload is outside the supported locator grammar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<Locator>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_hex: Option<String>,
    /// Three stored 32-bit words and their tags; semantics are not assigned.
    pub values: [i32; 3],
    pub value_tags: [u8; 3],
    pub shorts: [i16; 2],
    pub source: StreamSpan,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkInfo {
    pub state: MetadataState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_count: Option<i32>,
    #[serde(default)]
    pub items: Vec<DocumentLink>,
    /// Trailing 32-bit words of the table; retained without interpretation.
    #[serde(default)]
    pub index_table: Vec<u32>,
    /// Tag byte of those words: `0x24`, or `0x22` in the examined V5R12 files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_tag: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewInfo {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub components: u8,
    pub sof_marker: u8,
    pub source: StreamSpan,
    /// Four uninterpreted bytes before the JPEG in the CATPreview stream.
    pub stream_header_hex: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewCollection {
    pub state: MetadataState,
    pub items: Vec<PreviewInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataScope {
    pub stream_id: u32,
    pub grammar: String,
    pub state: MetadataState,
    /// Uninterpreted bytes remain recoverable from the input through these spans.
    pub opaque_ranges: Vec<StreamSpan>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataInfo {
    pub saved_by: VersionInfo,
    #[serde(default)]
    pub storage: StorageInfo,
    #[serde(default)]
    pub property_sets: Vec<PropertySet>,
    #[serde(default)]
    pub links: LinkInfo,
    pub previews: PreviewCollection,
    pub scopes: Vec<MetadataScope>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewImage {
    pub info: PreviewInfo,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
struct Issue {
    code: Code,
    offset: u64,
}
type Result<T> = std::result::Result<T, Issue>;
fn issue(code: Code, offset: u64) -> Issue {
    Issue { code, offset }
}
fn diagnostic(error: Issue, stream_id: u32, severity: Severity) -> Diagnostic {
    let mut d = Diagnostic::new(error.code, severity, None);
    d.location = Some(SourceLocation::LogicalStream {
        stream_id: stream_id.to_string(),
        offset: error.offset,
    });
    d
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn printable_ascii(bytes: &[u8]) -> bool {
    bytes.iter().all(|b| (0x20..0x7f).contains(b))
}

struct Budget<'a> {
    limits: &'a ParseLimits,
    bytes: u64,
    records: u64,
    steps: u64,
}
impl Budget<'_> {
    fn work(&mut self, count: u64, offset: u64) -> Result<()> {
        self.steps = self
            .steps
            .checked_add(count)
            .ok_or_else(|| issue(Code::MetadataStepLimitExceeded, offset))?;
        if self.steps > self.limits.max_metadata_steps {
            return Err(issue(Code::MetadataStepLimitExceeded, offset));
        }
        Ok(())
    }
    fn record(&mut self, offset: u64) -> Result<()> {
        self.records += 1;
        if self.records > self.limits.max_metadata_records {
            return Err(issue(Code::MetadataRecordLimitExceeded, offset));
        }
        self.work(1, offset)
    }
    fn remaining_records(&self) -> u64 {
        self.limits
            .max_metadata_records
            .saturating_sub(self.records)
    }
}

fn span(
    stream: &StreamInfo,
    start: u64,
    length: u64,
    budget: &mut Budget<'_>,
) -> Result<StreamSpan> {
    let end = start
        .checked_add(length)
        .filter(|v| *v <= stream.logical_length)
        .ok_or_else(|| issue(Code::PropertyInvalid, start))?;
    let mut ranges = Vec::new();
    for e in &stream.extents {
        budget.work(1, start)?;
        let left = start.max(e.logical_offset);
        let right = end.min(e.logical_offset + e.logical_length);
        if left < right {
            ranges.push(FileRange {
                offset: e.file_range.offset + left - e.logical_offset,
                length: right - left,
            });
        }
    }
    Ok(StreamSpan {
        stream_id: stream.id,
        logical_offset: start,
        length,
        file_ranges: ranges,
    })
}

struct Cursor<'a> {
    bytes: &'a [u8],
    reader: ByteReader<'a>,
}
impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8], limits: &ParseLimits) -> Result<Self> {
        Ok(Self {
            bytes,
            reader: ByteReader::new(bytes, 0, limits).map_err(|e| issue(e.code, e.offset))?,
        })
    }
    fn pos(&self) -> u64 {
        self.reader.position()
    }
    fn peek(&self, n: usize) -> Option<&'a [u8]> {
        let at = usize::try_from(self.pos()).ok()?;
        self.bytes.get(at..at.checked_add(n)?)
    }
    fn take(&mut self, n: u64) -> Result<&'a [u8]> {
        self.reader.read_bytes(n).map_err(|e| {
            issue(
                if e.code == Code::UnexpectedEof {
                    Code::PropertyInvalid
                } else {
                    e.code
                },
                e.offset,
            )
        })
    }
    fn expect(&mut self, bytes: &[u8], code: Code) -> Result<()> {
        let at = self.pos();
        if self.take(bytes.len() as u64)? != bytes {
            return Err(issue(code, at));
        }
        Ok(())
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn i16(&mut self) -> Result<i16> {
        let b = self.take(2)?;
        Ok(i16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn i32(&mut self) -> Result<i32> {
        let b = self.take(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    /// `u16` length followed by printable ASCII, as used by property sets.
    fn name(&mut self, budget: &mut Budget<'_>) -> Result<&'a [u8]> {
        let length = u64::from(self.u16()?);
        if length > budget.limits.max_property_bytes {
            return Err(issue(Code::PropertyLimitExceeded, self.pos()));
        }
        budget.work(length, self.pos())?;
        let name = self.take(length)?;
        if name.is_empty() || !printable_ascii(name) {
            return Err(issue(Code::MetadataScopeUnsupported, self.pos() - length));
        }
        Ok(name)
    }
    /// A string atom of the storage grammar: `34 <len> bytes`, or the
    /// observed length-free `20` form that carries exactly eight bytes.
    fn string_atom(&mut self, budget: &mut Budget<'_>) -> Result<(u64, &'a [u8])> {
        let at = self.pos();
        let length = match self.u8()? {
            0x34 => u64::from(self.u8()?),
            0x20 => 8,
            _ => return Err(issue(Code::StorageInvalid, at)),
        };
        budget.work(length, at)?;
        let start = self.pos();
        Ok((start, self.take(length)?))
    }
    fn tagged_i32(&mut self, tags: &[u8]) -> Result<(u8, i32)> {
        let at = self.pos();
        let tag = self.u8()?;
        if !tags.contains(&tag) {
            return Err(issue(Code::LinkInvalid, at));
        }
        Ok((tag, self.i32()?))
    }
}

fn version(bytes: &[u8]) -> Option<SavedVersion> {
    let mut rest = bytes;
    let mut fields = Vec::new();
    for name in ["Version", "Release", "ServicePack", "BuildDate", "HotFix"] {
        if name == "HotFix" && rest.is_empty() {
            // V5R12 tuples stop after the build date.
            break;
        }
        let open = format!("<{name}>");
        let close = format!("/<{name}>");
        rest = rest.strip_prefix(open.as_bytes())?;
        let end = rest.iter().position(|b| *b == b'/')?;
        let text = &rest[..end];
        if text.is_empty() || text.len() > 64 || !printable_ascii(text) || text.contains(&b'<') {
            return None;
        }
        if name != "BuildDate" && !text.iter().all(u8::is_ascii_digit) {
            return None;
        }
        fields.push(std::str::from_utf8(text).ok()?.to_owned());
        rest = rest[end..].strip_prefix(close.as_bytes())?;
    }
    if !rest.is_empty() {
        return None;
    }
    Some(SavedVersion {
        version: fields[0].clone(),
        release: fields[1].clone(),
        service_pack: fields[2].clone(),
        build_date: fields[3].clone(),
        hot_fix: fields.get(4).cloned(),
    })
}

fn plausible_name(bytes: &[u8]) -> bool {
    if bytes.len() < 2 {
        return false;
    }
    let length = usize::from(u16::from_le_bytes([bytes[0], bytes[1]]));
    (1..=255).contains(&length) && bytes.get(2..2 + length).is_some_and(printable_ascii)
}

/// A set body is a `u32` count followed by named properties. Sets declared by
/// name only (`CATPreview`) have no count and are followed by the next set
/// name or the end of the stream. The lookahead decides between the two forms
/// without reading past a plausible name; bytes that fit neither form are
/// invalid, and an oversized count is a resource rejection.
fn has_inline_body(bytes: &[u8], at: u64, limits: &ParseLimits) -> Result<bool> {
    let rest = usize::try_from(at)
        .ok()
        .and_then(|at| bytes.get(at..))
        .ok_or_else(|| issue(Code::PropertyInvalid, at))?;
    if rest.len() < 4 {
        return Ok(false);
    }
    let count = u32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]);
    if u64::from(count) <= limits.max_metadata_records && (count == 0 || plausible_name(&rest[4..]))
    {
        return Ok(true);
    }
    if plausible_name(rest) {
        return Ok(false);
    }
    Err(issue(
        if u64::from(count) > limits.max_metadata_records {
            Code::MetadataRecordLimitExceeded
        } else {
            Code::PropertyInvalid
        },
        at,
    ))
}

fn property_value(
    r: &mut Cursor<'_>,
    type_code: u32,
    depth: u8,
    budget: &mut Budget<'_>,
) -> Result<PropertyValue> {
    let at = r.pos();
    match type_code {
        2 => Ok(PropertyValue::U16(r.u16()?)),
        3 => Ok(PropertyValue::I32(r.i32()?)),
        13 | 14 => {
            let length = u64::from(r.u32()?);
            if length > budget.limits.max_property_bytes {
                return Err(issue(Code::PropertyLimitExceeded, at));
            }
            budget.work(length, at)?;
            let value = r.take(length)?;
            if type_code == 14 && printable_ascii(value) {
                Ok(PropertyValue::Text(
                    String::from_utf8(value.to_vec())
                        .map_err(|_| issue(Code::PropertyInvalid, at))?,
                ))
            } else {
                Ok(PropertyValue::Bytes(hex(value)))
            }
        }
        12 => {
            if depth > 0 {
                return Err(issue(Code::MetadataScopeUnsupported, at));
            }
            let count = r.u32()?;
            if u64::from(count) > budget.remaining_records() {
                return Err(issue(Code::MetadataRecordLimitExceeded, at));
            }
            let mut items = Vec::new();
            for _ in 0..count {
                budget.record(r.pos())?;
                let element_type = r.u32()?;
                items.push(property_value(r, element_type, depth + 1, budget)?);
            }
            Ok(PropertyValue::List(items))
        }
        _ => Err(issue(Code::MetadataScopeUnsupported, at)),
    }
}

fn property_sets(
    bytes: &[u8],
    stream: &StreamInfo,
    budget: &mut Budget<'_>,
    info: &mut MetadataInfo,
    scope: &mut MetadataScope,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let mut r = Cursor::new(bytes, budget.limits)?;
    if r.take(3)? != [1, 1, 0] {
        return Err(issue(Code::MetadataScopeUnsupported, 0));
    }
    let groups = r.u32()?;
    if groups == 0 {
        return Err(issue(Code::PropertyInvalid, 3));
    }
    if u64::from(groups) > budget.remaining_records() {
        return Err(issue(Code::MetadataRecordLimitExceeded, 3));
    }
    scope.grammar = "property_sets".into();
    let mut sets = Vec::new();
    for _ in 0..groups {
        let set_start = r.pos();
        budget.record(set_start)?;
        let set_name = r.name(budget)?;
        let summary = set_name == b"CATSummaryInformation";
        let inline = has_inline_body(bytes, r.pos(), budget.limits)?;
        let mut properties = Vec::new();
        if inline {
            let count = r.u32()?;
            if u64::from(count) > budget.remaining_records() {
                return Err(issue(Code::MetadataRecordLimitExceeded, r.pos() - 4));
            }
            for _ in 0..count {
                let start = r.pos();
                budget.record(start)?;
                let name = r.name(budget)?;
                let type_code = r.u32()?;
                let value_start = r.pos();
                let value = property_value(&mut r, type_code, 0, budget)?;
                if summary && name == b"LastSaveVersion" {
                    let raw = &bytes[value_start as usize + 4..r.pos() as usize];
                    match (type_code, version(raw)) {
                        (14, Some(value)) => {
                            info.saved_by.observations.push(VersionObservation {
                                value,
                                source: span(stream, start, r.pos() - start, budget)?,
                            });
                        }
                        _ => {
                            info.saved_by.state = MetadataState::Invalid;
                            scope.state = MetadataState::Invalid;
                            diagnostics.push(diagnostic(
                                issue(Code::VersionInvalid, start),
                                stream.id,
                                Severity::Warning,
                            ));
                        }
                    }
                }
                properties.push(Property {
                    name: String::from_utf8_lossy(name).into_owned(),
                    type_code,
                    value,
                    source: span(stream, start, r.pos() - start, budget)?,
                });
            }
        }
        sets.push(PropertySet {
            stream_id: stream.id,
            name: String::from_utf8_lossy(set_name).into_owned(),
            inline,
            properties,
            source: span(stream, set_start, r.pos() - set_start, budget)?,
        });
    }
    if r.pos() < stream.logical_length {
        scope.opaque_ranges.push(span(
            stream,
            r.pos(),
            stream.logical_length - r.pos(),
            budget,
        )?);
    }
    info.property_sets.extend(sets);
    Ok(())
}

const PREAMBLE: u64 = 126;
const STORAGE_ROOT: &[u8] = b"\x34\x12CATStorageProperty\x80\x01\0\0\0\0\x22\x0c\0\0\0\x34\x01";
const UNICODE_CLASS: &[u8] = b"\x34\x10CATUnicodeString";
const UNICODE_INSTANCE: &[u8] = b"\xa0\x02\0\0\0\0";
const TRAILER: &[u8] = b"\x36\x08FINJPL  ";
const DOCUMENT_ROOT: &[u8] = b"\x34\x13CATDocumentProperty\x80\x01\0\0\0\0";
const SYMBOL_CLASS: &[u8] = b"\x34\x11CATSymbolProperty";
const SYMBOL_INSTANCE: &[u8] = b"\xe0\x02\0\0\0\0";
const OCTET_CLASS: &[u8] = b"\x34\x0dCATOctetArray";
const OCTET_INSTANCE: &[u8] = b"\xe0\x03\0\0\0\0";
const END: u8 = 0x9f;

/// The envelope byte and the 126-byte preamble are observed, not decoded.
fn envelope(bytes: &[u8]) -> Result<u8> {
    match bytes.first() {
        Some(b @ (b'M' | b'N')) => Ok(*b),
        _ => Err(issue(Code::MetadataScopeUnsupported, 0)),
    }
}

fn storage(
    bytes: &[u8],
    stream: &StreamInfo,
    budget: &mut Budget<'_>,
    info: &mut MetadataInfo,
    scope: &mut MetadataScope,
) -> Result<()> {
    let envelope_byte = envelope(bytes)?;
    let root = usize::try_from(PREAMBLE).map_err(|_| issue(Code::OffsetOverflow, 0))?;
    if bytes.get(root..root + STORAGE_ROOT.len()) != Some(STORAGE_ROOT) {
        return Err(issue(Code::MetadataScopeUnsupported, 0));
    }
    let mut r = Cursor::new(bytes, budget.limits)?;
    r.take(PREAMBLE + STORAGE_ROOT.len() as u64)?;
    budget.record(PREAMBLE)?;
    let kind_byte = r.u8()?;
    r.expect(&[0], Code::StorageInvalid)?;
    let role = match kind_byte {
        1 => StorageRole::SavedName,
        2 => StorageRole::Document,
        3 => StorageRole::Container,
        _ => return Err(issue(Code::MetadataScopeUnsupported, r.pos() - 2)),
    };
    scope.grammar = match role {
        StorageRole::SavedName => "storage_saved_name",
        StorageRole::Document => "storage_document",
        StorageRole::Container => "storage_container",
    }
    .into();
    r.expect(UNICODE_CLASS, Code::MetadataScopeUnsupported)?;
    let mut values = Vec::new();
    for _ in 0..2 {
        r.expect(UNICODE_INSTANCE, Code::StorageInvalid)?;
        let (start, value) = r.string_atom(budget)?;
        r.expect(&[END], Code::StorageInvalid)?;
        let text =
            std::str::from_utf8(value).map_err(|_| issue(Code::MetadataScopeUnsupported, start))?;
        if text.bytes().any(|b| b < 0x20 || b == 0x7f) {
            return Err(issue(Code::StorageInvalid, start));
        }
        values.push((start, text.to_owned()));
    }
    if role == StorageRole::SavedName && values[0].1 != "CATIA" {
        return Err(issue(Code::MetadataScopeUnsupported, values[0].0));
    }
    let mut ids = Vec::new();
    while r.peek(1) == Some(&[0x3b]) {
        let at = r.pos();
        r.take(1)?;
        if r.u8()? != 16 {
            return Err(issue(Code::StorageInvalid, at));
        }
        budget.work(16, at)?;
        ids.push(hex(r.take(16)?));
    }
    if ids.is_empty() {
        return Err(issue(Code::StorageInvalid, r.pos()));
    }
    r.expect(&[END], Code::StorageInvalid)?;
    r.expect(TRAILER, Code::StorageInvalid)?;
    let (name_start, name) = values.pop().unwrap_or_default();
    let (_, type_name) = values.pop().unwrap_or_default();
    info.storage.declarations.push(StorageDeclaration {
        stream_id: stream.id,
        role,
        kind_byte,
        envelope_byte,
        name_source: span(stream, name_start, name.len() as u64, budget)?,
        type_name,
        name,
        ids_hex: ids,
        source: span(stream, PREAMBLE, r.pos() - PREAMBLE, budget)?,
    });
    scope.opaque_ranges.push(span(stream, 0, PREAMBLE, budget)?);
    if r.pos() < stream.logical_length {
        scope.opaque_ranges.push(span(
            stream,
            r.pos(),
            stream.logical_length - r.pos(),
            budget,
        )?);
    }
    Ok(())
}

/// Decode one locator payload: a length-prefixed identifier, a word, a
/// length-prefixed family name, the remaining lengths, `DocId`, `File`, the
/// document type and an optional path. Any deviation leaves the payload raw.
fn locator(payload: &[u8]) -> Option<Locator> {
    fn counted<'a>(payload: &'a [u8], at: &mut usize) -> Option<&'a [u8]> {
        let length = usize::from(*payload.get(*at)?);
        let value = payload.get(*at + 1..*at + 1 + length)?;
        *at += 1 + length;
        Some(value)
    }
    fn u32_at(payload: &[u8], at: &mut usize) -> Option<u32> {
        let b = payload.get(*at..*at + 4)?;
        *at += 4;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u16_at(payload: &[u8], at: &mut usize) -> Option<u16> {
        let b = payload.get(*at..*at + 2)?;
        *at += 2;
        Some(u16::from_le_bytes([b[0], b[1]]))
    }
    let mut at = 0;
    let id = counted(payload, &mut at)?;
    let word = u32_at(payload, &mut at)?;
    let kind = counted(payload, &mut at)?;
    if kind.is_empty() || !printable_ascii(kind) {
        return None;
    }
    let rest = u32_at(payload, &mut at)?;
    if rest as usize != payload.len() - at {
        return None;
    }
    if counted(payload, &mut at)? != b"DocId" {
        return None;
    }
    let rest = u16_at(payload, &mut at)?;
    if usize::from(rest) != payload.len() - at {
        return None;
    }
    if *payload.get(at)? != 1 {
        return None;
    }
    at += 1;
    if counted(payload, &mut at)? != b"File" {
        return None;
    }
    let document_type = counted(payload, &mut at)?;
    if document_type.is_empty() || !printable_ascii(document_type) {
        return None;
    }
    let (path, path_hex) = if at == payload.len() {
        (None, None)
    } else {
        let length = usize::from(u16_at(payload, &mut at)?);
        let value = payload.get(at..at + length)?;
        at += length;
        if at != payload.len() {
            return None;
        }
        match std::str::from_utf8(value) {
            Ok(text) => (Some(text.to_owned()), None),
            Err(_) => (None, Some(hex(value))),
        }
    };
    Some(Locator {
        kind: String::from_utf8_lossy(kind).into_owned(),
        id_hex: hex(id),
        word,
        document_type: String::from_utf8_lossy(document_type).into_owned(),
        path,
        path_hex,
    })
}

fn document_links(
    bytes: &[u8],
    stream: &StreamInfo,
    budget: &mut Budget<'_>,
    info: &mut MetadataInfo,
    scope: &mut MetadataScope,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    envelope(bytes)?;
    let root = usize::try_from(PREAMBLE).map_err(|_| issue(Code::OffsetOverflow, 0))?;
    if bytes.get(root..root + DOCUMENT_ROOT.len()) != Some(DOCUMENT_ROOT) {
        return Err(issue(Code::MetadataScopeUnsupported, 0));
    }
    scope.grammar = "document_links".into();
    let mut r = Cursor::new(bytes, budget.limits)?;
    r.take(PREAMBLE + DOCUMENT_ROOT.len() as u64)?;
    budget.record(PREAMBLE)?;
    r.tagged_i32(&[0x22])?;
    let (_, declared_count) = r.tagged_i32(&[0x22])?;
    info.links.declared_count = Some(declared_count);
    let mut items = Vec::new();
    loop {
        if r.peek(2) == Some(&[0, 0x34]) {
            r.take(1)?;
            r.expect(SYMBOL_CLASS, Code::LinkInvalid)?;
        }
        if r.peek(SYMBOL_INSTANCE.len()) != Some(SYMBOL_INSTANCE) {
            break;
        }
        let start = r.pos();
        budget.record(start)?;
        r.take(SYMBOL_INSTANCE.len() as u64)?;
        r.tagged_i32(&[0x22])?;
        if r.peek(2) == Some(&[0, 0x34]) {
            r.take(1)?;
            r.expect(OCTET_CLASS, Code::LinkInvalid)?;
        }
        r.expect(OCTET_INSTANCE, Code::LinkInvalid)?;
        let (_, length) = r.tagged_i32(&[0x22])?;
        let length = u64::try_from(length).map_err(|_| issue(Code::LinkInvalid, r.pos() - 4))?;
        if length > budget.limits.max_property_bytes {
            return Err(issue(Code::PropertyLimitExceeded, r.pos() - 4));
        }
        let mut payload = Vec::new();
        payload
            .try_reserve(length as usize)
            .map_err(|_| issue(Code::AllocationFailed, r.pos()))?;
        while (payload.len() as u64) < length {
            let at = r.pos();
            r.expect(&[0x3b], Code::LinkInvalid)?;
            let chunk = u64::from(r.u8()?);
            if chunk == 0 || payload.len() as u64 + chunk > length {
                return Err(issue(Code::LinkInvalid, at));
            }
            budget.work(chunk, at)?;
            payload.extend_from_slice(r.take(chunk)?);
        }
        r.expect(&[END], Code::LinkInvalid)?;
        let mut values = [0; 3];
        let mut value_tags = [0; 3];
        for (value, tag) in values.iter_mut().zip(value_tags.iter_mut()) {
            (*tag, *value) = r.tagged_i32(&[0x22, 0x23])?;
        }
        let mut shorts = [0; 2];
        for short in &mut shorts {
            r.expect(&[0x21], Code::LinkInvalid)?;
            *short = r.i16()?;
        }
        r.expect(&[END], Code::LinkInvalid)?;
        let decoded = locator(&payload);
        if decoded.is_none() {
            diagnostics.push(diagnostic(
                issue(Code::LinkInvalid, start),
                stream.id,
                Severity::Warning,
            ));
        }
        items.push(DocumentLink {
            ordinal: items.len() as u32,
            payload_hex: decoded.is_none().then(|| hex(&payload)),
            locator: decoded,
            values,
            value_tags,
            shorts,
            source: span(stream, start, r.pos() - start, budget)?,
        });
    }
    // V5R12 files tag the trailing words `22`; later files use `24`.
    let mut index_table = Vec::new();
    let mut index_tag = None;
    while let Some(&[tag @ (0x22 | 0x24)]) = r.peek(1) {
        if index_tag.is_some_and(|t| t != tag) {
            return Err(issue(Code::LinkInvalid, r.pos()));
        }
        index_tag = Some(tag);
        budget.record(r.pos())?;
        r.take(1)?;
        index_table.push(r.u32()?);
    }
    r.expect(&[END], Code::LinkInvalid)?;
    r.expect(TRAILER, Code::LinkInvalid)?;
    info.links.items.extend(items);
    info.links.index_table.extend(index_table);
    info.links.index_tag = index_tag;
    scope.opaque_ranges.push(span(stream, 0, PREAMBLE, budget)?);
    if r.pos() < stream.logical_length {
        scope.opaque_ranges.push(span(
            stream,
            r.pos(),
            stream.logical_length - r.pos(),
            budget,
        )?);
    }
    Ok(())
}

fn preview(
    bytes: &[u8],
    stream: &StreamInfo,
    budget: &mut Budget<'_>,
    info: &mut MetadataInfo,
    scope: &mut MetadataScope,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    budget.record(0)?;
    let jpeg = bytes
        .get(4..)
        .ok_or_else(|| issue(Code::PreviewInvalid, 0))?;
    let limits = ParseLimits {
        max_metadata_steps: budget
            .limits
            .max_metadata_steps
            .saturating_sub(budget.steps),
        ..*budget.limits
    };
    let image =
        inspect_jpeg_prefix(jpeg, &limits).map_err(|e| issue(e.code, 4 + e.offset as u64))?;
    budget.work(image.work_steps, 4)?;
    info.previews.items.push(PreviewInfo {
        id: info.previews.items.len() as u32,
        width: image.width,
        height: image.height,
        components: image.components,
        sof_marker: image.sof_marker,
        source: span(stream, 4, image.byte_length, budget)?,
        stream_header_hex: hex(&bytes[..4]),
    });
    scope.opaque_ranges.push(span(stream, 0, 4, budget)?);
    let end = 4 + image.byte_length;
    if end < stream.logical_length {
        scope
            .opaque_ranges
            .push(span(stream, end, stream.logical_length - end, budget)?);
        diagnostics.push(diagnostic(
            issue(Code::PreviewTrailingData, end),
            stream.id,
            Severity::Warning,
        ));
    }
    Ok(())
}

/// Mutates the report incrementally so resource failures retain earlier evidence.
pub(crate) fn parse(
    data: &[u8],
    container: &ContainerInfo,
    limits: &ParseLimits,
    info: &mut MetadataInfo,
    diagnostics: &mut Vec<Diagnostic>,
) -> std::result::Result<(), Box<Diagnostic>> {
    let mut budget = Budget {
        limits,
        bytes: 0,
        records: 0,
        steps: 0,
    };
    for stream in &container.streams {
        if container.directories[stream.directory_id as usize].kind != DirectoryKind::Outer {
            continue;
        }
        let name = stream.name.as_deref().unwrap_or("");
        if !matches!(
            name,
            "CATPropertySets" | "CATStorageProperty" | "CATDocumentProperty" | "CATPreview"
        ) {
            continue;
        }
        let mut scope = MetadataScope {
            stream_id: stream.id,
            grammar: name.into(),
            state: MetadataState::Present,
            opaque_ranges: Vec::new(),
        };
        let attempt = (|| {
            budget.record(0)?;
            budget.bytes = budget
                .bytes
                .checked_add(stream.logical_length)
                .ok_or_else(|| issue(Code::MetadataLimitExceeded, 0))?;
            if budget.bytes > limits.max_metadata_bytes {
                return Err(issue(Code::MetadataLimitExceeded, 0));
            }
            if name == "CATPreview"
                && stream.logical_length.saturating_sub(4) > limits.max_preview_bytes
            {
                return Err(issue(Code::PreviewLimitExceeded, 4));
            }
            Ok(())
        })();
        if let Err(error) = attempt {
            return Err(Box::new(diagnostic(error, stream.id, Severity::Error)));
        }
        let bytes = reconstruct_stream(data, stream, limits)?;
        let result = match name {
            "CATPropertySets" => {
                property_sets(&bytes, stream, &mut budget, info, &mut scope, diagnostics)
            }
            "CATStorageProperty" => storage(&bytes, stream, &mut budget, info, &mut scope),
            "CATDocumentProperty" => {
                document_links(&bytes, stream, &mut budget, info, &mut scope, diagnostics)
            }
            _ => preview(&bytes, stream, &mut budget, info, &mut scope, diagnostics),
        };
        if let Err(error) = result {
            if error.code.is_resource_limit() {
                return Err(Box::new(diagnostic(error, stream.id, Severity::Error)));
            }
            let state = if matches!(
                error.code,
                Code::MetadataScopeUnsupported | Code::PreviewUnsupported
            ) {
                MetadataState::Unsupported
            } else {
                MetadataState::Invalid
            };
            scope.state = state;
            // Preserve the complete scope when the partial grammar cannot close.
            scope.opaque_ranges.clear();
            scope.opaque_ranges.push(
                span(stream, 0, stream.logical_length, &mut budget)
                    .map_err(|e| Box::new(diagnostic(e, stream.id, Severity::Error)))?,
            );
            match name {
                "CATPropertySets" => mark_state(&mut info.saved_by.state, state),
                "CATStorageProperty" => mark_state(&mut info.storage.state, state),
                "CATDocumentProperty" => mark_state(&mut info.links.state, state),
                _ => mark_state(&mut info.previews.state, state),
            }
            diagnostics.push(diagnostic(error, stream.id, Severity::Warning));
        }
        info.scopes.push(scope);
    }
    finish(info, diagnostics);
    Ok(())
}

/// Governing values are assigned only when every admitted observation agrees.
fn finish(info: &mut MetadataInfo, diagnostics: &mut Vec<Diagnostic>) {
    let observations = &info.saved_by.observations;
    let conflict = observations
        .first()
        .is_some_and(|first| observations.iter().any(|o| o.value != first.value));
    if conflict {
        info.saved_by.state = MetadataState::Conflicting;
        diagnostics.push(Diagnostic::new(
            Code::VersionConflict,
            Severity::Warning,
            None,
        ));
    } else if !matches!(
        info.saved_by.state,
        MetadataState::Invalid | MetadataState::Unsupported
    ) && let Some(first) = observations.first()
    {
        info.saved_by.state = MetadataState::Present;
        info.saved_by.value = Some(first.value.clone());
    }

    let storage = &mut info.storage;
    let mut conflicts = false;
    let documents: Vec<_> = storage
        .declarations
        .iter()
        .filter(|d| d.role == StorageRole::Document)
        .collect();
    if let Some(first) = documents.first() {
        if documents
            .iter()
            .all(|d| d.type_name == first.type_name && d.name == first.name)
        {
            storage.document = Some(DocumentDeclaration {
                type_name: first.type_name.clone(),
                name: first.name.clone(),
                document_id_hex: first.ids_hex.last().cloned().unwrap_or_default(),
                stream_id: first.stream_id,
                source: first.source.clone(),
            });
        } else {
            conflicts = true;
        }
    }
    let names: Vec<_> = storage
        .declarations
        .iter()
        .filter(|d| d.role == StorageRole::SavedName)
        .collect();
    if let Some(first) = names.first() {
        if names.iter().all(|d| d.name == first.name) {
            storage.saved_name = Some(SavedName {
                name: first.name.clone(),
                stream_id: first.stream_id,
                source: first.name_source.clone(),
            });
        } else {
            conflicts = true;
        }
    }
    if conflicts {
        storage.state = MetadataState::Conflicting;
        diagnostics.push(Diagnostic::new(
            Code::StorageConflict,
            Severity::Warning,
            None,
        ));
    } else if !storage.declarations.is_empty() && !matches!(storage.state, MetadataState::Invalid) {
        storage.state = MetadataState::Present;
    }

    if !matches!(
        info.links.state,
        MetadataState::Invalid | MetadataState::Unsupported
    ) && (!info.links.items.is_empty() || info.links.declared_count.is_some())
    {
        info.links.state = MetadataState::Present;
    }
    if !matches!(
        info.previews.state,
        MetadataState::Invalid | MetadataState::Unsupported
    ) && !info.previews.items.is_empty()
    {
        info.previews.state = MetadataState::Present;
    }
}

fn mark_state(current: &mut MetadataState, new: MetadataState) {
    if *current != MetadataState::Invalid {
        *current = new;
    }
}
