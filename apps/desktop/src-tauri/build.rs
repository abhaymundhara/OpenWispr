use std::env;
use std::process::Command;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os == "windows" {
        let has_clang = Command::new("clang").arg("--version").output().is_ok();
        let has_vulkan = env::var("VULKAN_SDK").is_ok();

        if has_clang && has_vulkan {
            println!("cargo:rustc-cfg=feature=\"vulkan\"");
            println!("cargo:warning=Hardware acceleration (Vulkan) detected and enabled.");
        } else {
            if !has_clang {
                println!("cargo:warning=LLVM/Clang not found. Hardware acceleration will be disabled.");
            }
            if !has_vulkan {
                println!("cargo:warning=Vulkan SDK not found. Hardware acceleration will be disabled.");
            }
            println!("cargo:warning=Building in CPU-only mode for maximum compatibility.");
        }
    }

    tauri_build::build();
}
