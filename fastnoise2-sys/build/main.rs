use std::{env, path::PathBuf, process::Command};

const SOURCE_DIR_KEY: &str = "FASTNOISE2_SOURCE_DIR";
const LIB_DIR_KEY: &str = "FASTNOISE2_LIB_DIR";
const BINDINGS_CACHE_KEY: &str = "FASTNOISE2_BINDINGS_DIR";
const LIB_NAME: &str = "FastNoise";
const HEADER_NAME: &str = "FastNoise_C.h";

// GitHub repo for prebuilt WASM binaries
// TODO: Change to upstream repo when merged
const WASM_PREBUILT_REPO: &str = "api-haus/fastnoise2-rs";
const WASM_PREBUILT_TAG: &str = "wasm-prebuilt-v1";

fn main() {
  if env::var("DOCS_RS").is_ok() {
    println!("cargo:warning=docs.rs compilation detected, only bindings will be generated");
    generate_bindings(default_source_path());
    return;
  }

  println!("cargo:rerun-if-env-changed={SOURCE_DIR_KEY}");
  println!("cargo:rerun-if-env-changed={LIB_DIR_KEY}");
  println!("cargo:rerun-if-env-changed={BINDINGS_CACHE_KEY}");
  println!("cargo:rerun-if-env-changed=EMSDK");

  let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();

  // WASM builds use pure WASM with SIMD128
  if target_arch == "wasm32" {
    build_wasm();
    return; // WASM doesn't need C++ stdlib linking
  }

  // Native builds follow existing logic
  let feature_build_from_source = env::var("CARGO_FEATURE_BUILD_FROM_SOURCE").is_ok();

  if feature_build_from_source {
    println!(
      "cargo:warning=feature 'build-from-source' is enabled; building FastNoise2 from source"
    );
    build_from_source();
  } else if let Ok(lib_dir) = env::var(LIB_DIR_KEY) {
    println!("cargo:warning=using precompiled library located in '{lib_dir}'");
    println!("cargo:rustc-link-search=native={lib_dir}");
    println!("cargo:rustc-link-lib=static={LIB_NAME}");

    generate_bindings(default_source_path());
  } else {
    println!("cargo:warning={LIB_DIR_KEY} is not set; falling back to building from source");
    build_from_source();
  }

  emit_std_cpp_link();
}

fn build_wasm() {
  // Try to use prebuilt WASM binaries first (no Emscripten needed!)
  if let Some(prebuilt_path) = try_download_wasm_prebuilt() {
    println!("cargo:warning=Using prebuilt WASM binaries from GitHub releases");
    let lib_path = prebuilt_path.clone();
    println!("cargo:rustc-link-search=native={}", lib_path.display());
    println!("cargo:rustc-link-lib=static={LIB_NAME}");

    // Use the prebuilt bindings
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    let bindings_src = prebuilt_path.join("bindings.rs");
    let bindings_dst = out_path.join("bindings.rs");
    if bindings_src.exists() {
      std::fs::copy(&bindings_src, &bindings_dst).expect("Failed to copy prebuilt bindings");
    }
    return;
  }

  // Fall back to building from source with Emscripten
  println!("cargo:warning=Prebuilt not available, building FastNoise2 for WASM with Emscripten");
  build_wasm_from_source();
}

/// Try to download prebuilt WASM binaries from GitHub releases
/// Returns Some(path) if successful, None if download failed
fn try_download_wasm_prebuilt() -> Option<PathBuf> {
  // Skip if user explicitly wants to build from source
  if env::var("FASTNOISE2_BUILD_WASM_FROM_SOURCE").is_ok() {
    println!("cargo:warning=FASTNOISE2_BUILD_WASM_FROM_SOURCE set, skipping prebuilt download");
    return None;
  }

  let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
  let prebuilt_dir = out_dir.join("wasm-prebuilt");
  let lib_file = prebuilt_dir.join("libFastNoise.a");

  // Already downloaded?
  if lib_file.exists() {
    println!("cargo:warning=Using cached WASM prebuilt from {}", prebuilt_dir.display());
    return Some(prebuilt_dir);
  }

  // Download from GitHub releases
  let url = format!(
    "https://github.com/{}/releases/download/{}/fastnoise2-wasm-prebuilt.tar.gz",
    WASM_PREBUILT_REPO, WASM_PREBUILT_TAG
  );

  println!("cargo:warning=Downloading WASM prebuilt from {}", url);

  // Create prebuilt directory
  std::fs::create_dir_all(&prebuilt_dir).ok()?;

  let tarball_path = out_dir.join("fastnoise2-wasm-prebuilt.tar.gz");

  // Try curl first, then wget
  let download_result = Command::new("curl")
    .args(["-fsSL", "-o", tarball_path.to_str().unwrap(), &url])
    .status()
    .ok()
    .filter(|s| s.success())
    .or_else(|| {
      Command::new("wget")
        .args(["-q", "-O", tarball_path.to_str().unwrap(), &url])
        .status()
        .ok()
        .filter(|s| s.success())
    });

  if download_result.is_none() {
    println!("cargo:warning=Failed to download WASM prebuilt (curl/wget failed or release not found)");
    return None;
  }

  // Extract tarball
  let extract_result = Command::new("tar")
    .args(["-xzf", tarball_path.to_str().unwrap(), "-C", prebuilt_dir.to_str().unwrap()])
    .status()
    .ok()
    .filter(|s| s.success());

  if extract_result.is_none() {
    println!("cargo:warning=Failed to extract WASM prebuilt tarball");
    return None;
  }

  // Verify extraction
  if !lib_file.exists() {
    println!("cargo:warning=WASM prebuilt archive missing libFastNoise.a");
    return None;
  }

  println!("cargo:warning=Successfully downloaded WASM prebuilt to {}", prebuilt_dir.display());
  Some(prebuilt_dir)
}

fn build_wasm_from_source() {
  let source_path = env::var(SOURCE_DIR_KEY)
    .map(PathBuf::from)
    .unwrap_or_else(|_| default_source_path());

  println!("cargo:warning=Building FastNoise2 for WASM with SIMD128 support");

  // Log the EMSDK path if set (for debugging)
  if let Ok(emsdk) = env::var("EMSDK") {
    println!("cargo:warning=EMSDK path: {}", emsdk);
  }
  println!(
    "cargo:rerun-if-changed={}",
    source_path.join("include").join("FastNoise").display()
  );

  // Get Emscripten SDK path from environment
  let emsdk_path = env::var("EMSDK")
    .expect("EMSDK environment variable required for WASM builds. Install from https://emscripten.org or use prebuilt binaries");

  // Use Emscripten's CMake toolchain file - this properly configures compilers and sysroot
  let toolchain_file = format!("{}/upstream/emscripten/cmake/Modules/Platform/Emscripten.cmake", emsdk_path);

  // Build FastNoise2 for WASM as a pure static library using Emscripten toolchain
  // FastSIMD has native WASM SIMD128 support - we just need to enable it
  let mut config = cmake::Config::new(&source_path);
  config
    .profile("Release")
    .define("CMAKE_TOOLCHAIN_FILE", &toolchain_file)
    .define("FASTNOISE2_TOOLS", "OFF")
    .define("FASTNOISE2_TESTS", "OFF")
    .define("FASTNOISE2_UTILITY", "OFF")  // Disable utility to avoid Corrade dependency
    .define("BUILD_SHARED_LIBS", "OFF");

  // Enable WASM SIMD128 only (no threading/atomics for compatibility with simple WASM demos)
  // NOTE: If Rust is built with --shared-memory, FastNoise2 also needs atomics (-pthread)
  // For now, keep it simple and let individual projects add atomics if needed
  let wasm_flags = "-msimd128";
  config.define("CMAKE_C_FLAGS", wasm_flags);
  config.define("CMAKE_CXX_FLAGS", wasm_flags);

  let out_path = config.build();
  let lib_path = out_path.join("lib");
  let lib64_path = out_path.join("lib64");

  println!("cargo:rustc-link-search=native={}", lib_path.display());
  println!("cargo:rustc-link-search=native={}", lib64_path.display());
  println!("cargo:rustc-link-lib=static={LIB_NAME}");

  // Copy Utility headers that cmake doesn't install
  let src_utility = source_path
    .join("include")
    .join("FastNoise")
    .join("Utility");
  let dst_utility = out_path.join("include").join("FastNoise").join("Utility");
  if src_utility.exists() && !dst_utility.exists() {
    std::fs::create_dir_all(&dst_utility).expect("Failed to create Utility dir");
    for entry in std::fs::read_dir(&src_utility).expect("Failed to read Utility dir") {
      let entry = entry.expect("Failed to read entry");
      let dst = dst_utility.join(entry.file_name());
      std::fs::copy(entry.path(), &dst).expect("Failed to copy header");
    }
  }

  generate_bindings(out_path);
}

fn build_from_source() {
  let source_path = env::var(SOURCE_DIR_KEY)
    .map(PathBuf::from)
    .unwrap_or_else(|_| default_source_path());

  println!(
    "cargo:warning=building from source files located in '{}'",
    source_path.display()
  );
  println!(
    "cargo:rerun-if-changed={}",
    source_path.join("include").join("FastNoise").display()
  );

  let mut config = cmake::Config::new(&source_path);
  config
    .profile("Release")
    .define("FASTNOISE2_TOOLS", "OFF")
    .define("FASTNOISE2_TESTS", "OFF")
    .define("FASTNOISE2_UTILITY", "OFF")
    .define("BUILD_SHARED_LIBS", "OFF");

  // https://github.com/rust-lang/cmake-rs/issues/198:
  // cmake-rs add default arguments such as CMAKE_CXX_FLAGS_RELEASE to the build
  // command. Removing these would automatically add Release profile args to
  // allow for better execution performance. FastNoise2 wiki steps for compiling the library (https://github.com/Auburn/FastNoise2/wiki/Compiling-FastNoise2):
  // 1. cmake -S . -B build -D FASTNOISE2_NOISETOOL=OFF -D FASTNOISE2_TESTS=OFF -D
  //    BUILD_SHARED_LIBS=OFF
  // 2. cmake --build build --config Release
  // This give us optimized build (with MSVC):
  // -> build/CMakeCache.txt: CMAKE_CXX_FLAGS_RELEASE:STRING=/MD /O2 /Ob2 /DNDEBUG
  // Whereas when using cmake-rs:
  // -> build/CMakeCache.txt: CMAKE_CXX_FLAGS_RELEASE:STRING= -nologo -MD -Brepro
  // Replace default arguments with those from the FastNoise2 manual build

  // Set optimization flags based on the target
  let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
  let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap();

  let cmake_cxx_flags_release = match (target_os.as_str(), target_env.as_str()) {
    ("windows", "msvc") => "/MD /O2 /Ob2 /DNDEBUG",
    // For GCC/Clang (Linux, macOS, MinGW)
    _ => "-O3 -DNDEBUG",
  };

  println!(
    "cargo:warning=CARGO_CFG_TARGET_OS='{target_os}' and CARGO_CFG_TARGET_ENV='{target_env}' => \
     CMAKE_CXX_FLAGS_RELEASE='{cmake_cxx_flags_release}'"
  );
  config.define("CMAKE_CXX_FLAGS_RELEASE", cmake_cxx_flags_release);

  let out_path = config.build();
  let lib_path = out_path.join("lib");
  let lib64_path = out_path.join("lib64");

  println!("cargo:rustc-link-search=native={}", lib_path.display());
  println!("cargo:rustc-link-search=native={}", lib64_path.display());
  println!("cargo:rustc-link-lib=static={LIB_NAME}");

  // Copy Utility headers that cmake doesn't install
  let src_utility = source_path
    .join("include")
    .join("FastNoise")
    .join("Utility");
  let dst_utility = out_path.join("include").join("FastNoise").join("Utility");
  if src_utility.exists() && !dst_utility.exists() {
    std::fs::create_dir_all(&dst_utility).expect("Failed to create Utility dir");
    for entry in std::fs::read_dir(&src_utility).expect("Failed to read Utility dir") {
      let entry = entry.expect("Failed to read entry");
      let dst = dst_utility.join(entry.file_name());
      std::fs::copy(entry.path(), &dst).expect("Failed to copy header");
    }
  }

  generate_bindings(out_path);
}

fn generate_bindings(source_path: PathBuf) {
  let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
  let bindings_path = out_path.join("bindings.rs");

  // For WASM builds, use vendored bindings (bindgen has issues with WASM target)
  // The C API bindings are platform-agnostic anyway (pure extern "C" declarations)
  let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
  if target_arch == "wasm32" {
    let vendored_bindings = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
      .join("src")
      .join("bindings_vendored.rs");

    if vendored_bindings.exists() {
      println!(
        "cargo:warning=using vendored bindings for WASM from '{}'",
        vendored_bindings.display()
      );
      std::fs::copy(&vendored_bindings, &bindings_path)
        .expect("Failed to copy vendored bindings");
      return;
    } else {
      println!("cargo:warning=vendored bindings not found, will attempt to generate");
    }
  }

  // Check for cached bindings
  if let Ok(cache_dir) = env::var(BINDINGS_CACHE_KEY) {
    let cached_bindings = PathBuf::from(&cache_dir).join("bindings.rs");
    if cached_bindings.exists() {
      println!(
        "cargo:warning=using cached bindings from '{}'",
        cached_bindings.display()
      );
      std::fs::copy(&cached_bindings, &bindings_path).expect("Failed to copy cached bindings");
      return;
    }
  }

  println!(
    "cargo:warning=generating Rust bindings for FastNoise2 (this is slow, set \
     FASTNOISE2_BINDINGS_DIR to cache)"
  );

  let include_path = source_path.join("include").join("FastNoise");
  let header_path = include_path.join(HEADER_NAME);

  // FastNoise C API bindings are target-agnostic (pure extern "C" declarations)
  // Generate them without target-specific flags for compatibility
  let bindings = bindgen::Builder::default()
    .header(header_path.to_str().unwrap())
    // Add include path for relative includes like "Utility/Export.h"
    .clang_arg(format!("-I{}", include_path.to_str().unwrap()))
    // 'bool' exists in C++ but not directly in C, it is named _Bool or you can use 'bool' by
    // including 'stdbool.h'
    .clang_arg("-xc++")
    // Parse as C-compatible for maximum portability
    .clang_arg("-fno-exceptions")
    .generate()
    .expect("Unable to generate bindings");

  bindings
    .write_to_file(&bindings_path)
    .expect("Couldn't write bindings!");

  println!(
    "cargo:warning=bindings generated successfully and written to '{}'",
    bindings_path.display()
  );

  // Save to cache if dir is set
  if let Ok(cache_dir) = env::var(BINDINGS_CACHE_KEY) {
    let cache_path = PathBuf::from(&cache_dir);
    std::fs::create_dir_all(&cache_path).ok();
    let cached_bindings = cache_path.join("bindings.rs");
    std::fs::copy(&bindings_path, &cached_bindings).ok();
    println!(
      "cargo:warning=bindings cached to '{}'",
      cached_bindings.display()
    );
  }
}

fn default_source_path() -> PathBuf {
  let mut path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
  path.push("build");
  path.push("FastNoise2");
  path
}

fn emit_std_cpp_link() {
  let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
  let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap();

  match (target_os.as_str(), target_env.as_str()) {
    ("linux", _) | ("windows", "gnu") => println!("cargo:rustc-link-lib=dylib=stdc++"),
    ("macos" | "freebsd", _) => println!("cargo:rustc-link-lib=dylib=c++"),
    ("windows", "msvc") => {} // MSVC links C++ stdlib automatically
    _ => println!("cargo:warning=Unknown target for C++ stdlib linking"),
  }
}
