fn main() {
    println!("cargo:rerun-if-env-changed=DECAPOD_CONSTITUTION_DIR");
    println!("cargo:rerun-if-changed=constitution");
}
