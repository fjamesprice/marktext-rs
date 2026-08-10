//! E2 build 1 — parley + `vello_cpu` only, no wgpu/vello, no tiny-skia.
//!
//! This binary IS `e2_common::run_and_print_baseline`; it exists as its own
//! crate (rather than folding into `e2-common`) so that `cargo build -p
//! e2-vello-cpu --profile dist` produces build 1's own `.exe`, separately
//! sized from the "+X" builds that depend on this same baseline.
fn main() {
    e2_common::run_and_print_baseline("vello_cpu");
}
