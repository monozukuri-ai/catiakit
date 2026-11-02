//! Borrowed reads with checked ranges, explicit endianness, and file positions.
use crate::{Diagnostic, DiagnosticCode, ParseLimits, Severity};
use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadError {
    pub code: DiagnosticCode,
    pub offset: u64,
    pub expected_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
    pub limit_bytes: Option<u64>,
}

impl ReadError {
    /// Allocate the user-facing diagnostic only when a read error is reported.
    #[must_use]
    pub fn into_diagnostic(self) -> Diagnostic {
        let mut diagnostic = Diagnostic::new(self.code, Severity::Error, Some(self.offset));
        diagnostic.details.expected_bytes = self.expected_bytes;
        diagnostic.details.available_bytes = self.available_bytes;
        diagnostic.details.limit_bytes = self.limit_bytes;
        diagnostic
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at file offset {}", self.code.message(), self.offset)
    }
}

impl Error for ReadError {}

fn error(code: DiagnosticCode, offset: u64) -> ReadError {
    ReadError {
        code,
        offset,
        expected_bytes: None,
        available_bytes: None,
        limit_bytes: None,
    }
}

#[derive(Debug, Clone)]
pub struct ByteReader<'a> {
    data: &'a [u8],
    position: usize,
    base_offset: u64,
    max_read_bytes: u64,
}

impl<'a> ByteReader<'a> {
    /// `base_offset` is the file position of the first byte in `data`.
    pub fn new(data: &'a [u8], base_offset: u64, limits: &ParseLimits) -> Result<Self, ReadError> {
        let length = u64::try_from(data.len())
            .map_err(|_| error(DiagnosticCode::OffsetOverflow, base_offset))?;
        base_offset
            .checked_add(length)
            .ok_or_else(|| error(DiagnosticCode::OffsetOverflow, base_offset))?;
        Ok(Self {
            data,
            position: 0,
            base_offset,
            max_read_bytes: limits.max_read_bytes,
        })
    }

    #[must_use]
    pub fn position(&self) -> u64 {
        self.position as u64
    }

    #[must_use]
    pub fn file_offset(&self) -> u64 {
        // Construction validates the end of the complete view.
        self.base_offset + self.position as u64
    }

    #[must_use]
    pub fn remaining(&self) -> u64 {
        (self.data.len() - self.position) as u64
    }

    /// Failed reads leave the position unchanged. An empty read at EOF is valid.
    pub fn read_bytes(&mut self, length: u64) -> Result<&'a [u8], ReadError> {
        let range = self.checked_range(self.position, length)?;
        self.position = range.end;
        Ok(&self.data[range])
    }

    pub fn seek(&mut self, position: u64) -> Result<(), ReadError> {
        let position = usize::try_from(position)
            .map_err(|_| error(DiagnosticCode::OffsetOverflow, self.file_offset()))?;
        if position > self.data.len() {
            return Err(error(DiagnosticCode::InvalidPosition, self.file_offset()));
        }
        self.position = position;
        Ok(())
    }

    /// Create a bounded child view without moving the parent cursor.
    pub fn subreader(&self, offset: u64, length: u64) -> Result<Self, ReadError> {
        let offset = usize::try_from(offset)
            .map_err(|_| error(DiagnosticCode::OffsetOverflow, self.base_offset))?;
        let range = self.checked_range(offset, length)?;
        let base_offset = self.base_offset + range.start as u64;
        Ok(Self {
            data: &self.data[range],
            position: 0,
            base_offset,
            max_read_bytes: self.max_read_bytes,
        })
    }

    fn checked_range(
        &self,
        start: usize,
        length: u64,
    ) -> Result<std::ops::Range<usize>, ReadError> {
        let offset = self
            .base_offset
            .checked_add(start as u64)
            .ok_or_else(|| error(DiagnosticCode::OffsetOverflow, self.base_offset))?;
        if length > self.max_read_bytes {
            let mut failure = error(DiagnosticCode::ReadLimitExceeded, offset);
            failure.expected_bytes = Some(length);
            failure.limit_bytes = Some(self.max_read_bytes);
            return Err(failure);
        }
        let length =
            usize::try_from(length).map_err(|_| error(DiagnosticCode::OffsetOverflow, offset))?;
        let end = start
            .checked_add(length)
            .ok_or_else(|| error(DiagnosticCode::OffsetOverflow, offset))?;
        if start > self.data.len() || end > self.data.len() {
            let mut failure = error(DiagnosticCode::UnexpectedEof, offset);
            failure.expected_bytes = Some(length as u64);
            failure.available_bytes = Some(self.data.len().saturating_sub(start) as u64);
            return Err(failure);
        }
        Ok(start..end)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], ReadError> {
        let bytes = self.read_bytes(N as u64)?;
        let mut result = [0; N];
        result.copy_from_slice(bytes);
        Ok(result)
    }

    pub fn read_u8(&mut self) -> Result<u8, ReadError> {
        Ok(self.array::<1>()?[0])
    }

    pub fn read_u16_le(&mut self) -> Result<u16, ReadError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub fn read_u16_be(&mut self) -> Result<u16, ReadError> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    pub fn read_u32_le(&mut self) -> Result<u32, ReadError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub fn read_u32_be(&mut self) -> Result<u32, ReadError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    pub fn read_u64_le(&mut self) -> Result<u64, ReadError> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    pub fn read_u64_be(&mut self) -> Result<u64, ReadError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    pub fn read_f32_le(&mut self) -> Result<f32, ReadError> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    pub fn read_f64_le(&mut self) -> Result<f64, ReadError> {
        Ok(f64::from_le_bytes(self.array()?))
    }
}
