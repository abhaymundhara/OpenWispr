use std::env;
use std::process::Command;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    // Windows: Check for Vulkan hardware acceleration
    if target_os == "windows" {
        let has_clang = Command::new("clang").arg("--version").output().is_ok();
        let has_vulkan = env::var("VULKAN_SDK").is_ok();

        if has_clang && has_vulkan {
            println!("cargo:rustc-cfg=feature=\"vulkan\"");
            println!("cargo:warning=Hardware acceleration (Vulkan) detected and enabled.");
        } else {
            if !has_clang {
                println!(
                    "cargo:warning=LLVM/Clang not found. Hardware acceleration will be disabled."
                );
            }
            if !has_vulkan {
                println!(
                    "cargo:warning=Vulkan SDK not found. Hardware acceleration will be disabled."
                );
            }
            println!("cargo:warning=Building in CPU-only mode for maximum compatibility.");
        }
    }

    // macOS: Configure rpath for dynamic dependencies
    #[cfg(target_os = "macos")]
    {
        // sherpa-rs-sys dynamic deps (e.g. libonnxruntime*.dylib) are copied
        // next to the executable in target/<profile>. Expose that directory via rpath.
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");
    }

    tauri_build::build();
}
