//! UniFFI's bindings generator, built from this workspace so its version always matches the
//! `uniffi` the library was compiled with. The scripts call it in library mode on a built
//! `libbandear_ffi`, which carries the exported interface in its metadata.

fn main() {
    uniffi::uniffi_bindgen_main()
}
