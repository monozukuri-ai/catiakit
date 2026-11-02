#![no_main]
use catiakit_core::{ParseLimits, ParseOptions, container::reconstruct_stream, inspect};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let options = ParseOptions {
        limits: ParseLimits {
            max_input_bytes: 4 * 1024 * 1024,
            max_directory_bytes: 64 * 1024,
            max_streams: 256,
            max_extents: 1024,
            max_stream_bytes: 1024 * 1024,
            max_total_stream_bytes: 8 * 1024 * 1024,
            max_nesting_depth: 3,
            max_scan_steps: 128 * 1024,
            max_name_bytes: 512,
            ..ParseLimits::default()
        },
    };
    let outcome = inspect(data, &options);
    if let Some(container) = outcome.value.and_then(|v| v.container) {
        // Also exercise retained outer evidence after a nested parse failure.
        for stream in &container.streams {
            let output = reconstruct_stream(data, stream, &options.limits).unwrap();
            assert_eq!(output.len() as u64, stream.logical_length);
        }
    }
});
