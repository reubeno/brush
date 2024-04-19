fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=patches/rustyline.patch");
    cargo_patch::patch().expect("Failed while patching");
}
