#![doc = "Builds the private C++ ABI bridge and embedded Format7zF provider."]

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const SDK_MAKEFILE: &str = "../../cmpl_gcc.mak";
const SDK_WARNING_FLAGS: &str = "-Werror -Wall -Wextra -Wno-error=array-bounds";

fn main() {
    println!("cargo:rerun-if-changed=native/pulp_7z_bridge.cpp");
    println!("cargo:rerun-if-changed=native/pulp_7z_bridge.h");
    println!("cargo:rerun-if-changed=native/sdk");
    println!("cargo:rerun-if-env-changed=AR");
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rerun-if-env-changed=CXX");

    let target_os = env::var("CARGO_CFG_TARGET_OS").expect("Cargo must provide the target OS");
    assert_eq!(
        target_os, "linux",
        "pulp currently embeds the Linux Format7zF build only"
    );

    let sdk = Path::new("native/sdk");
    let bundle = sdk.join("CPP/7zip/Bundles/Format7zF");
    for required in [
        sdk.join("CPP/7zip/cmpl_gcc.mak"),
        sdk.join("CPP/7zip/Archive/IArchive.h"),
        sdk.join("CPP/7zip/IStream.h"),
        bundle.join("makefile.gcc"),
    ] {
        assert!(
            required.exists(),
            "the 7-Zip SDK submodule is missing {}; initialize crates/pulp/native/sdk",
            required.display()
        );
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must provide OUT_DIR"));
    let sdk_out = out_dir.join("7zip-sdk");
    build_sdk(&bundle, &sdk_out);

    let objects = collect_objects(&sdk_out);
    let archive = sdk_out.join("libpulp_7z_sdk.a");
    archive_objects(&archive, &objects);

    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .file("native/pulp_7z_bridge.cpp")
        .include("native")
        .include("native/sdk")
        .warnings(true)
        .compile("pulp_7z_bridge");

    println!("cargo:rustc-link-search=native={}", sdk_out.display());
    // These are the native dependencies used by the Linux SDK bundle. The C++
    // standard library is emitted by cc for the C++ bridge itself.
    println!("cargo:rustc-link-lib=dylib=pthread");
    println!("cargo:rustc-link-lib=dylib=dl");
}

fn build_sdk(bundle: &Path, output: &Path) {
    let jobs = env::var("NUM_JOBS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|jobs| *jobs > 0)
        .unwrap_or(1);
    let cc = env::var("CC").unwrap_or_else(|_| "cc".to_owned());
    let cxx = env::var("CXX").unwrap_or_else(|_| "c++".to_owned());

    let status = Command::new("make")
        .current_dir(bundle)
        .arg(format!("-j{jobs}"))
        .arg("-f")
        .arg(SDK_MAKEFILE)
        .arg(format!("O={}", output.display()))
        .arg(format!("CC={cc}"))
        .arg(format!("CXX={cxx}"))
        .arg(format!("CFLAGS_WARN_WALL={SDK_WARNING_FLAGS}"))
        .status()
        .unwrap_or_else(|error| panic!("failed to run the 7-Zip SDK build: {error}"));

    assert!(
        status.success(),
        "the official 7-Zip Format7zF build failed with status {status}"
    );
}

fn collect_objects(output: &Path) -> Vec<PathBuf> {
    let mut objects = fs::read_dir(output)
        .unwrap_or_else(|error| {
            panic!(
                "failed to read 7-Zip build output {}: {error}",
                output.display()
            )
        })
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("o"))
        .collect::<Vec<_>>();
    objects.sort();
    assert!(
        !objects.is_empty(),
        "the official 7-Zip Format7zF build produced no object files in {}",
        output.display()
    );
    objects
}

fn archive_objects(output: &Path, objects: &[PathBuf]) {
    if output.exists() {
        fs::remove_file(output).unwrap_or_else(|error| {
            panic!(
                "failed to replace generated static archive {}: {error}",
                output.display()
            )
        });
    }

    let ar = env::var("AR").unwrap_or_else(|_| "ar".to_owned());
    let status = Command::new(&ar)
        .arg("crs")
        .arg(output)
        .args(objects)
        .status()
        .unwrap_or_else(|error| panic!("failed to run {ar} for the 7-Zip static archive: {error}"));
    assert!(
        status.success(),
        "the 7-Zip static archive command failed with status {status}"
    );
}
