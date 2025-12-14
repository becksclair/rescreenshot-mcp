//! Encoder performance benchmarks
//!
//! Measures encoding time for PNG, JPEG, and WebP formats on 4K (3840x2160) buffers.
//! This helps identify encoding bottlenecks on the critical path.

use criterion::{Criterion, criterion_group, criterion_main};
use screenshot_core::{
    capture::ImageBuffer,
    model::{CaptureOptions, ImageFormat},
    util::encode::encode_image,
};
use std::hint::black_box;

fn create_4k_test_image() -> ImageBuffer {
    // Create a 4K test image (3840x2160)
    ImageBuffer::from_test_pattern(3840, 2160)
}

fn bench_png_encoding_fast(c: &mut Criterion) {
    let img = create_4k_test_image();
    let opts = CaptureOptions::builder()
        .format(ImageFormat::Png)
        .quality(80) // Default quality maps to Fast compression
        .build();

    c.bench_function("encode_png_fast_4k", |b| {
        b.iter(|| {
            encode_image(black_box(&img), black_box(&opts)).unwrap();
        });
    });
}

fn bench_png_encoding_default(c: &mut Criterion) {
    let img = create_4k_test_image();
    let opts = CaptureOptions::builder()
        .format(ImageFormat::Png)
        .quality(90) // Maps to Default compression
        .build();

    c.bench_function("encode_png_default_4k", |b| {
        b.iter(|| {
            encode_image(black_box(&img), black_box(&opts)).unwrap();
        });
    });
}

fn bench_png_encoding_best(c: &mut Criterion) {
    let img = create_4k_test_image();
    let opts = CaptureOptions::builder()
        .format(ImageFormat::Png)
        .quality(100) // Maps to Best compression
        .build();

    c.bench_function("encode_png_best_4k", |b| {
        b.iter(|| {
            encode_image(black_box(&img), black_box(&opts)).unwrap();
        });
    });
}

fn bench_jpeg_encoding(c: &mut Criterion) {
    let img = create_4k_test_image();
    let opts = CaptureOptions::builder()
        .format(ImageFormat::Jpeg)
        .quality(80)
        .build();

    c.bench_function("encode_jpeg_4k", |b| {
        b.iter(|| {
            encode_image(black_box(&img), black_box(&opts)).unwrap();
        });
    });
}

fn bench_webp_encoding(c: &mut Criterion) {
    let img = create_4k_test_image();
    let opts = CaptureOptions::builder()
        .format(ImageFormat::Webp)
        .quality(80)
        .build();

    c.bench_function("encode_webp_4k", |b| {
        b.iter(|| {
            encode_image(black_box(&img), black_box(&opts)).unwrap();
        });
    });
}

criterion_group!(
    benches,
    bench_png_encoding_fast,
    bench_png_encoding_default,
    bench_png_encoding_best,
    bench_jpeg_encoding,
    bench_webp_encoding
);
criterion_main!(benches);
