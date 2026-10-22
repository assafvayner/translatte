use std::path::{Path, PathBuf};

/// Layers the PP-OCRv5 det/rec models use, plus those NCNN creates internally while running them
/// (activations fused into convolutions, layout and fp16 conversions, Gemm-based convolutions)
/// and what the tests' tiny model uses. Every other layer is left out of the build.
const LAYERS: &[&str] = &[
    "BinaryOp",
    "Bias",
    "Cast",
    "Clip",
    "Concat",
    "Convolution",
    "Convolution1D",
    "ConvolutionDepthWise",
    "Crop",
    "Deconvolution",
    "DeconvolutionDepthWise",
    "Dequantize",
    "Eltwise",
    "Flatten",
    "Gemm",
    "HardSigmoid",
    "HardSwish",
    "InnerProduct",
    "Input",
    "Interp",
    "LayerNorm",
    "MemoryData",
    "Mish",
    "MultiHeadAttention",
    "Noop",
    "Packing",
    "Padding",
    "Permute",
    "Pooling",
    "Quantize",
    "ReLU",
    "Requantize",
    "Reshape",
    "Scale",
    "SDPA",
    "Sigmoid",
    "Slice",
    "Softmax",
    "Split",
    "Squeeze",
    "Swish",
];

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let target = std::env::var("TARGET").unwrap();
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let src = dir.join("vendor/ncnn");
    println!("cargo:rerun-if-changed=vendor");
    let tracked: &[&str] = match os.as_str() {
        "android" => &["ANDROID_NDK", "ANDROID_NDK_HOME", "ANDROID_API_LEVEL"],
        "ios" => &["IPHONEOS_DEPLOYMENT_TARGET"],
        _ => &[],
    };
    for var in tracked {
        println!("cargo:rerun-if-env-changed={var}");
    }

    let mut cfg = cmake::Config::new(&src);
    cfg.profile("Release")
        .define("NCNN_VULKAN", "OFF")
        .define("NCNN_BUILD_TOOLS", "OFF")
        .define("NCNN_BUILD_EXAMPLES", "OFF")
        .define("NCNN_BUILD_BENCHMARK", "OFF")
        .define("NCNN_BUILD_TESTS", "OFF")
        .define("NCNN_PYTHON", "OFF")
        .define("NCNN_SHARED_LIB", "OFF")
        .define("NCNN_C_API", "ON")
        .define("NCNN_PIXEL", "OFF")
        .define("NCNN_PIXEL_ROTATE", "OFF")
        .define("NCNN_PIXEL_AFFINE", "OFF")
        .define("NCNN_PIXEL_DRAWING", "OFF")
        .build_target("ncnn");
    for layer in all_layers(&src) {
        if !LAYERS.contains(&layer.as_str()) {
            cfg.define(format!("WITH_LAYER_{}", layer.to_lowercase()), "OFF");
        }
    }
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
            .define("ANDROID_STL", "c++_shared")
            .define("NCNN_OPENMP", "ON");
            link_ndk_openmp(Path::new(&ndk));
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
                .define("CMAKE_SYSTEM_PROCESSOR", "arm64")
                .define("CMAKE_OSX_SYSROOT", sysroot)
                .define("CMAKE_OSX_DEPLOYMENT_TARGET", min)
                .define("NCNN_OPENMP", "ON")
                .define("NCNN_SIMPLEOMP", "ON");
        }
        "macos" => {
            cfg.define("NCNN_OPENMP", "ON")
                .define("NCNN_SIMPLEOMP", "ON");
        }
        _ => {
            cfg.define("NCNN_OPENMP", "OFF");
        }
    }
    let out = cfg.build();
    let lib_dir = out.join("build/src");
    assert!(
        lib_dir.join("libncnn.a").exists(),
        "the CMake build produced no libncnn.a in {}",
        lib_dir.display()
    );
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=ncnn");
    let system: &[&str] = match os.as_str() {
        "android" => &["c++_shared", "log", "dl", "m"],
        "ios" | "macos" => &["c++"],
        _ => &["stdc++", "pthread"],
    };
    for lib in system {
        println!("cargo:rustc-link-lib=dylib={lib}");
    }
}

/// Every layer the vendored `src/CMakeLists.txt` registers with `ncnn_add_layer(Name)`.
fn all_layers(src: &Path) -> Vec<String> {
    let cmake = std::fs::read_to_string(src.join("src/CMakeLists.txt")).unwrap();
    cmake
        .lines()
        .filter_map(|l| l.trim().strip_prefix("ncnn_add_layer("))
        .filter_map(|l| l.split(')').next())
        .map(|l| l.trim().to_string())
        .collect()
}

/// Links the NDK's static LLVM OpenMP runtime, which NCNN's `#pragma omp` loops call into.
fn link_ndk_openmp(ndk: &Path) {
    let prebuilt = ndk.join("toolchains/llvm/prebuilt");
    let host = std::fs::read_dir(&prebuilt)
        .unwrap_or_else(|e| panic!("reading {}: {e}", prebuilt.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .expect("no NDK host toolchain");
    let clang = host.join("lib/clang");
    let lib = std::fs::read_dir(&clang)
        .unwrap_or_else(|e| panic!("reading {}: {e}", clang.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path().join("lib/linux/aarch64"))
        .find(|p| p.join("libomp.a").exists())
        .expect("no libomp.a for aarch64 in the NDK");
    println!("cargo:rustc-link-search=native={}", lib.display());
    println!("cargo:rustc-link-lib=static=omp");
}
