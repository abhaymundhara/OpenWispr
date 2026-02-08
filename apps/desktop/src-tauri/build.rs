fn main() {
  #[cfg(target_os = "macos")]
  {
    // sherpa-rs-sys dynamic deps (e.g. libonnxruntime*.dylib) are copied
    // next to the executable in target/<profile>. Expose that directory via rpath.
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");
  }
  tauri_build::build()
}
