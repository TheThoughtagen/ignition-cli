//! Placeholder main — the real dispatch chassis (try_parse → env defaults →
//! tracing → dispatch → single ExitCode) lands in Task 2 of this plan.

fn main() {
    println!("ign {}", env!("CARGO_PKG_VERSION"));
}
