//! Integration test that invokes the differential harness binary.
//! Requires `luau` on PATH; if not present, the test is skipped.

use std::process::Command;

#[test]
fn run_differential_harness() {
    if which("luau").is_err() {
        eprintln!("skipping: `luau` not on PATH");
        return;
    }
    let status = Command::new(env!("CARGO_BIN_EXE_luau-obf-difftest"))
        .status()
        .expect("spawn harness");
    assert!(status.success(), "differential harness failed");
}

fn which(prog: &str) -> Result<(), ()> {
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            let p = std::path::Path::new(dir).join(prog);
            if p.exists() {
                return Ok(());
            }
        }
    }
    Err(())
}
