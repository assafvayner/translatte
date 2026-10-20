use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let target = std::env::var("TARGET").unwrap();
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let arch = std::env::var("BERGAMOT_BUILD_ARCH").ok();
    println!("cargo:rerun-if-changed=native");
    println!("cargo:rerun-if-changed=vendor");
    // Only the variables this target reads, so a host build (e.g. uniffi-bindgen run from the
    // Android scripts) doesn't rebuild when the NDK settings come and go.
    let tracked: &[&str] = match os.as_str() {
        "android" => &["ANDROID_NDK", "ANDROID_NDK_HOME", "ANDROID_API_LEVEL"],
        "ios" => &["IPHONEOS_DEPLOYMENT_TARGET"],
        _ => &[],
    };
    for var in tracked.iter().chain(&["BERGAMOT_BUILD_ARCH"]) {
        println!("cargo:rerun-if-env-changed={var}");
    }

    let mut cfg = cmake::Config::new(dir.join("native"));
    // Marian is far too slow unoptimized, so even debug builds of the crate use a release build.
    cfg.profile("Release")
        .define("BERGAMOT_SRC", dir.join("vendor/inference"))
        // As an environment variable so it also reaches the cmake that configures pcre2.
        .env("CMAKE_POLICY_VERSION_MINIMUM", "3.5")
        .define("GIT_SUBMODULE", "OFF")
        .define("SSPLIT_USE_INTERNAL_PCRE2", "ON")
        .define("USE_STATIC_LIBS", "ON")
        .define("USE_DOXYGEN", "OFF")
        .define("USE_MKL", "OFF")
        .define("USE_NCCL", "OFF")
        .build_target("bergamot_shim");
    match os.as_str() {
        "android" => {
            let ndk = std::env::var("ANDROID_NDK")
                .or_else(|_| std::env::var("ANDROID_NDK_HOME"))
                .expect("set ANDROID_NDK (scripts/android/env.sh does)");
            let api = std::env::var("ANDROID_API_LEVEL").unwrap_or_else(|_| "31".into());
            cfg.define(
                "CMAKE_TOOLCHAIN_FILE",
                Path::new(&ndk).join("build/cmake/android.toolchain.cmake"),
            )
            .define("ANDROID_ABI", "arm64-v8a")
            .define("ANDROID_PLATFORM", format!("android-{api}"))
            // ONNX Runtime's AAR already needs libc++_shared.so, which the app and CLI ship.
            .define("ANDROID_STL", "c++_shared")
            .define("BUILD_ARCH", arch.as_deref().unwrap_or("armv8.2-a+dotprod"));
        }
        "ios" => {
            let sysroot = if target.ends_with("-sim") {
                "iphonesimulator"
            } else {
                "iphoneos"
            };
            let min = std::env::var("IPHONEOS_DEPLOYMENT_TARGET").unwrap_or_else(|_| "17.0".into());
            cfg.define("CMAKE_SYSTEM_NAME", "iOS")
                .define("CMAKE_OSX_ARCHITECTURES", "arm64")
                .define("CMAKE_OSX_SYSROOT", sysroot)
                .define("CMAKE_OSX_DEPLOYMENT_TARGET", min)
                // iOS 17 still runs on A12 phones, which lack the dot-product extension.
                .define("BUILD_ARCH", arch.as_deref().unwrap_or("armv8-a"));
        }
        _ => {
            cfg.define("BUILD_ARCH", arch.as_deref().unwrap_or("native"));
        }
    }
    let out = cfg.build();
    link_archives(&out.join("build"));
    let system: &[&str] = match os.as_str() {
        "android" => &["c++_shared", "log", "android", "dl", "atomic", "m"],
        "ios" | "macos" => {
            println!("cargo:rustc-link-lib=framework=Accelerate");
            &["c++", "iconv"]
        }
        _ => &["stdc++"],
    };
    for lib in system {
        println!("cargo:rustc-link-lib=dylib={lib}");
    }
}

/// Links every static library the CMake build produced. Unused ones cost nothing: the linker only
/// pulls referenced objects, and lld and ld64 resolve symbols regardless of archive order.
fn link_archives(build: &Path) {
    let mut seen = HashSet::new();
    let mut dirs = vec![build.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let Some(lib) = name.strip_prefix("lib").and_then(|n| n.strip_suffix(".a")) else {
                continue;
            };
            if seen.insert(lib.to_string()) {
                println!("cargo:rustc-link-search=native={}", dir.display());
                println!("cargo:rustc-link-lib=static={lib}");
            }
        }
    }
    for lib in [
        "bergamot_shim",
        "bergamot-translator-source",
        "marian",
        "sentencepiece",
        "ssplit",
        "pcre2-8",
    ] {
        assert!(seen.contains(lib), "the CMake build produced no lib{lib}.a");
    }
}
