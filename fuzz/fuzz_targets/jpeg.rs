#![no_main]
use catiakit_core::{
    ParseLimits,
    jpeg::{inspect_jpeg, inspect_jpeg_prefix},
};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let limits = ParseLimits {
        max_preview_bytes: 128 * 1024,
        max_preview_pixels: 1024 * 1024,
        max_metadata_steps: 512 * 1024,
        ..ParseLimits::default()
    };
    if let Ok(info) = inspect_jpeg_prefix(data, &limits) {
        let image = &data[..info.byte_length as usize];
        assert_eq!(
            inspect_jpeg(image, &limits).unwrap().byte_length,
            image.len() as u64
        );
    }
});
