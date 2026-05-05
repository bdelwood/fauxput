use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");

    // libxcvt is the canonical CVT/CVT-RB implementation
    // Let's use it instead of worrying about (re)implementing our own timing math
    let lib = pkg_config::Config::new()
        .atleast_version("0.1")
        .probe("libxcvt")
        .unwrap_or_else(|e| {
            panic!(
                "libxcvt not found. Install libxcvt (Arch: \
                 `pacman -S libxcvt`, Debian: `apt install libxcvt-dev`, \
                 Fedora: `dnf install libxcvt-devel`).\n\nDetails: {e}"
            )
        });

    let bindings = bindgen::Builder::default()
        .header_contents("wrapper.h", "#include <libxcvt/libxcvt.h>")
        .clang_args(
            lib.include_paths
                .iter()
                .map(|p| format!("-I{}", p.display())),
        )
        .allowlist_function("libxcvt_.*")
        .allowlist_type("libxcvt_.*")
        .allowlist_var("LIBXCVT_.*")
        .prepend_enum_name(false)
        .derive_debug(true)
        .derive_default(true)
        .derive_copy(true)
        .generate_comments(false)
        .generate()
        .expect("bindgen failed");

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    bindings
        .write_to_file(out.join("libxcvt.rs"))
        .expect("write bindings");

    for path in lib.include_paths {
        println!("cargo:include={}", path.display());
    }
}
