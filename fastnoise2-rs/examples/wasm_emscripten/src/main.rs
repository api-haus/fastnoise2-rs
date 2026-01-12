//! FastNoise2 WASM Demo using wasm32-unknown-emscripten target
//!
//! This uses Emscripten's native JS glue code instead of wasm-bindgen,
//! which is required for mixing Rust and C++ code in WebAssembly.

use fastnoise2::generator::prelude::*;
use fastnoise2::SafeNode;
use std::alloc::{alloc, dealloc, Layout};

/// Create a simple noise generator using the typed API
fn create_noise_node() -> GeneratorWrapper<SafeNode> {
    supersimplex().build()
}

/// Allocate memory in WASM linear memory
/// Called from JavaScript to allocate buffers
#[no_mangle]
pub extern "C" fn wasm_alloc(size: usize) -> *mut u8 {
    let layout = Layout::from_size_align(size, 1).unwrap();
    unsafe { alloc(layout) }
}

/// Free memory in WASM linear memory
/// Called from JavaScript to free buffers
#[no_mangle]
pub extern "C" fn wasm_free(ptr: *mut u8, size: usize) {
    let layout = Layout::from_size_align(size, 1).unwrap();
    unsafe { dealloc(ptr, layout) }
}

/// Generate 2D noise and write grayscale values (0-255) to output buffer
/// Returns 0 on success, -1 on failure
#[no_mangle]
pub extern "C" fn generate_noise_2d(
    output: *mut u8,
    width: i32,
    height: i32,
    seed: i32,
) -> i32 {
    let size = (width * height) as usize;

    // Create noise generator using typed API
    let node = create_noise_node();

    // Generate float noise values
    let step_size = 0.05;
    let mut float_output = vec![0.0f32; size];
    node.gen_uniform_grid_2d(
        &mut float_output,
        0.0,       // x_offset
        0.0,       // y_offset
        width,     // x_count
        height,    // y_count
        step_size, // x_step_size
        step_size, // y_step_size
        seed,
    );

    // Convert to grayscale bytes and write to output buffer
    let output_slice = unsafe { std::slice::from_raw_parts_mut(output, size) };
    for (i, &v) in float_output.iter().enumerate() {
        output_slice[i] = ((v + 1.0) * 0.5 * 255.0).clamp(0.0, 255.0) as u8;
    }

    0 // success
}

/// Generate 3D noise slice and write grayscale values (0-255) to output buffer
/// Returns 0 on success, -1 on failure
#[no_mangle]
pub extern "C" fn generate_noise_3d_slice(
    output: *mut u8,
    width: i32,
    height: i32,
    z_offset: f32,
    seed: i32,
) -> i32 {
    let size = (width * height) as usize;

    let node = create_noise_node();
    let step_size = 0.05;

    let mut float_output = vec![0.0f32; size];
    node.gen_uniform_grid_3d(
        &mut float_output,
        0.0,           // x_offset
        0.0,           // y_offset
        z_offset,      // z_offset
        width,         // x_count
        height,        // y_count
        1,             // z_count
        step_size,     // x_step_size
        step_size,     // y_step_size
        step_size,     // z_step_size
        seed,
    );

    let output_slice = unsafe { std::slice::from_raw_parts_mut(output, size) };
    for (i, &v) in float_output.iter().enumerate() {
        output_slice[i] = ((v + 1.0) * 0.5 * 255.0).clamp(0.0, 255.0) as u8;
    }

    0 // success
}

/// Simple test function - returns the SIMD level to verify WASM is working
/// Returns -1 on failure, otherwise returns SIMD level * 1000
#[no_mangle]
pub extern "C" fn test_fastnoise2() -> i32 {
    let node = create_noise_node();

    let simd_level = node.get_simd_level();

    let mut output = vec![0.0f32; 100];
    node.gen_uniform_grid_2d(&mut output, 0.0, 0.0, 10, 10, 0.1, 0.1, 1337);

    // Return SIMD level * 1000 + first value indicator
    (simd_level as i32 * 1000) + ((output[0] + 1.0) * 100.0) as i32
}

/// Entry point for emscripten - called when module loads
/// The exported functions above remain callable from JavaScript
fn main() {
    // Module initialization complete
    // All #[no_mangle] extern "C" functions are now available to JavaScript
}
