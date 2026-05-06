//! Code chunk header parsing for BEAM files.
//!
//! The Code chunk contains a 40-byte header followed by the actual bytecode.

use super::iff::LoadError as IffLoadError;

/// Code chunk header layout (40 bytes)
pub const CODE_HEADER_SIZE: usize = 40;

/// Magic value for code chunk header
pub const CODE_HEADER_MAGIC: u32 = 0x7F_00_00_00;

/// A parsed code chunk header
#[derive(Debug, Clone)]
pub struct CodeHeader {
    pub magic: u32,
    pub version: u32,
    pub flags: u32,
    pub code_size: u32,
    pub export_count: u32,
    pub import_count: u32,
    pub local_count: u32,
    pub lambda_count: u32,
    pub code_label_count: u32,
    pub function_count: u32,
}

/// Parse code chunk header
pub fn parse_code_header(data: &[u8]) -> Result<CodeHeader, LoadError> {
    if data.len() < CODE_HEADER_SIZE {
        return Err(LoadError::CodePayloadTooShort {
            expected: CODE_HEADER_SIZE,
            got: data.len(),
        });
    }

    let mut offset = 0;

    let magic = u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]);
    offset += 4;

    if magic != CODE_HEADER_MAGIC {
        return Err(LoadError::InvalidCodeHeader("bad magic"));
    }

    let version = u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]);
    offset += 4;

    let flags = u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]);
    offset += 4;

    let code_size = u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]);
    offset += 4;

    let export_count = u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]);
    offset += 4;

    let import_count = u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]);
    offset += 4;

    let local_count = u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]);
    offset += 4;

    let lambda_count = u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]);
    offset += 4;

    let code_label_count = u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]);
    offset += 4;

    let function_count = u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]);

    Ok(CodeHeader {
        magic,
        version,
        flags,
        code_size,
        export_count,
        import_count,
        local_count,
        lambda_count,
        code_label_count,
        function_count,
    })
}

/// Load error types for code header parsing
#[derive(Debug)]
pub enum LoadError {
    CodePayloadTooShort { expected: usize, got: usize },
    InvalidCodeHeader(&'static str),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::CodePayloadTooShort { expected, got } => {
                write!(
                    f,
                    "code payload too short: expected {} got {}",
                    expected, got
                )
            }
            LoadError::InvalidCodeHeader(s) => {
                write!(f, "invalid code header: {}", s)
            }
        }
    }
}

impl std::error::Error for LoadError {}

impl From<IffLoadError> for LoadError {
    fn from(_: IffLoadError) -> Self {
        LoadError::InvalidCodeHeader("IFF error")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_header_parse() {
        let mut data = vec![0u8; CODE_HEADER_SIZE];
        data[0..4].copy_from_slice(&CODE_HEADER_MAGIC.to_be_bytes());
        data[4..8].copy_from_slice(&0u32.to_be_bytes());
        data[12..16].copy_from_slice(&100u32.to_be_bytes());
        data[16..20].copy_from_slice(&2u32.to_be_bytes());

        let header = parse_code_header(&data).unwrap();
        assert_eq!(header.magic, CODE_HEADER_MAGIC);
        assert_eq!(header.code_size, 100);
        assert_eq!(header.export_count, 2);
    }

    #[test]
    fn test_code_header_too_short() {
        let data = vec![0u8; 20];
        let result = parse_code_header(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_code_header_bad_magic() {
        let mut data = vec![0u8; CODE_HEADER_SIZE];
        data[0..4].copy_from_slice(&0xDEADBEEFu32.to_be_bytes());

        let result = parse_code_header(&data);
        assert!(result.is_err());
    }
}
