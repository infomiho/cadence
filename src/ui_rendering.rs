#![allow(
    dead_code,
    unused_imports,
    reason = "The native harness shares production modules without running their unit tests."
)]

include!("lib.rs");

fn main() {
    app::test_support::rendering::run();
}
