#![no_main]
use catiakit_core::{
    MetadataState, ParseLimits, ParseOptions, ParseStatus, extract_preview, inspect,
};
use libfuzzer_sys::fuzz_target;

// Keep CFV2 framing intact while fuzzing a single named metadata payload.
// Seed byte selects CATPropertySets, CATStorageProperty, CATDocumentProperty,
// or CATPreview.
fuzz_target!(|data: &[u8]| {
    if data.len() < 2 || data.len() > 128 * 1024 {
        return;
    }
    let name = match data[0] % 4 {
        0 => "CATPropertySets",
        1 => "CATStorageProperty",
        2 => "CATDocumentProperty",
        _ => "CATPreview",
    };
    let payload = &data[1..];
    let mut image = b"V5_CFV2\0".to_vec();
    image.resize(64, 0);
    image.extend_from_slice(payload);
    let directory = image.len();
    image.extend_from_slice(b"CATIA_V5 CB0001\0");
    image.extend_from_slice(&[0; 32]);
    for c in name.encode_utf16() {
        image.extend_from_slice(&c.to_le_bytes());
    }
    image.extend_from_slice(&[0; 3]);
    let ds = image.len();
    image.resize(ds + 84, 0);
    image[ds + 12..ds + 16].copy_from_slice(&(payload.len() as u32).to_be_bytes());
    image[ds + 80..ds + 84].copy_from_slice(&1u32.to_be_bytes());
    for word in [64, payload.len() as u32, payload.len() as u32, 0, 0] {
        image.extend_from_slice(&word.to_be_bytes());
    }
    image.extend_from_slice(b"CB__END\0");
    let length = image.len() - directory;
    image[8..12].copy_from_slice(&(directory as u32).to_be_bytes());
    image[12..16].copy_from_slice(&(length as u32).to_be_bytes());
    let options = ParseOptions {
        limits: ParseLimits {
            max_input_bytes: 256 * 1024,
            max_metadata_bytes: 128 * 1024,
            max_metadata_steps: 512 * 1024,
            max_metadata_records: 128,
            max_property_bytes: 16 * 1024,
            max_preview_bytes: 128 * 1024,
            max_preview_pixels: 1024 * 1024,
            max_streams: 16,
            max_extents: 32,
            ..ParseLimits::default()
        },
    };
    let outcome = inspect(&image, &options);
    if outcome.report.status != ParseStatus::Rejected {
        let metadata = outcome.value.unwrap().metadata.unwrap();
        if metadata.previews.state == MetadataState::Present {
            let preview = extract_preview(&image, None, &options).value.unwrap();
            assert_eq!(preview.bytes.len() as u64, preview.info.source.length);
        }
    }
});
