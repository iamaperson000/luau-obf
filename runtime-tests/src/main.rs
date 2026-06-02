//! Differential test harness.
//!
//! Walks `runtime-tests/corpus/*.luau`, obfuscates each, and compares execution
//! output to the plain Luau interpreter's output. Requires `luau` on PATH.
// AH! tests are here. good to know
// Just an idea, maybe we have it add tests by giving an input that has a defentiive output (i.e. 5+5) and run nning it and the obfusctaed version to see if they differ? idk
// cool
use luau_obf::{obfuscate, Options};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn main() {
    let corpus_dir = corpus_dir();
    let entries: Vec<PathBuf> = match fs::read_dir(&corpus_dir) {
        Ok(rd) => rd
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension() == Some(OsStr::new("luau")))
            .collect(),
        Err(e) => {
            eprintln!("could not read corpus dir {corpus_dir:?}: {e}");
            std::process::exit(2);
        }
    };
    if entries.is_empty() {
        eprintln!("no .luau files found in {corpus_dir:?}");
        std::process::exit(2);
    }
    let mut failed: Vec<String> = Vec::new();
    let mut passed = 0usize;
    for path in entries {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        match run_one(&path) {
            Ok(()) => {
                println!("PASS  {name}");
                passed += 1;
            }
            Err(msg) => {
                println!("FAIL  {name}\n      {msg}");
                failed.push(name);
            }
        }
    }
    println!();
    println!("{} passed, {} failed", passed, failed.len());
    if !failed.is_empty() {
        std::process::exit(1);
    }
}

fn run_one(source_path: &Path) -> Result<(), String> {
    let src = fs::read_to_string(source_path).map_err(|e| format!("read source: {e}"))?;
    let plain = run_luau_with_source(&src)?;

    let seed = [0xABu8; 32];
    let r1 = obfuscate(&src, Options { seed: Some(seed) })
        .map_err(|e| format!("obfuscate (run 1): {e}"))?;
    let r2 = obfuscate(&src, Options { seed: Some(seed) })
        .map_err(|e| format!("obfuscate (run 2): {e}"))?;
    if r1.output != r2.output {
        return Err("non-deterministic obfuscator output for fixed seed".into());
    }

    let obfuscated_output = run_luau_with_source(&r1.output)?;
    if plain != obfuscated_output {
        return Err(format!(
            "output mismatch:\n  plain: {plain:?}\n  obfus: {obfuscated_output:?}"
        ));
    }
    Ok(())
}

fn run_luau_with_source(source: &str) -> Result<String, String> {
    let dir = tempdir()?;
    let path = dir.join("run.luau");
    fs::write(&path, source).map_err(|e| format!("write tmp: {e}"))?;
    let out = Command::new("luau")
        .arg(&path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("spawn luau: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        return Err(format!("luau exited non-zero: {} stderr={stderr}", out.status));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn corpus_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir).join("corpus")
}

fn tempdir() -> Result<PathBuf, String> {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let path = base.join(format!("luau-obf-difftest-{pid}-{nanos}"));
    fs::create_dir_all(&path).map_err(|e| format!("mkdir tmp: {e}"))?;
    Ok(path)
}
