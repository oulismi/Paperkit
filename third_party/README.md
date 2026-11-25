# Vendored crates

## mupdf (0.8.0)

Copy of the `mupdf` crate from crates.io (AGPL-3.0, https://github.com/messense/mupdf-rs),
used through `[patch.crates-io]` in the root `Cargo.toml`. One change, in
`src/device/native.rs`: on Windows `max_align_t` is missing from the generated
bindings, so the alignment check uses `f64` (which is what `max_align_t` is on MSVC).

Remove this folder and the patch entry once an upstream release builds on Windows.
