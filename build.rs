fn main() {
    if let Some(path) = std::env::var_os("LIBKRUN_EFI") {
        println!("cargo:rustc-link-search={}", path.to_string_lossy());
    } else {
        println!("cargo:rustc-link-search=/opt/homebrew/lib");
    }
    println!("cargo:rustc-link-lib=krun-efi");
}
