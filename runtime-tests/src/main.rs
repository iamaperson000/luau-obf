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

    // Determinism check against the canonical seed.
    let canonical_seed = [0xABu8; 32];
    let r1 = obfuscate(&src, Options { seed: Some(canonical_seed) })
        .map_err(|e| format!("obfuscate (run 1): {e}"))?;
    let r2 = obfuscate(&src, Options { seed: Some(canonical_seed) })
        .map_err(|e| format!("obfuscate (run 2): {e}"))?;
    if r1.output != r2.output {
        return Err("non-deterministic obfuscator output for fixed seed".into());
    }

    // Multi-seed differential: every chosen seed must yield bit-identical
    // stdout to the plain Luau interpreter. Catches per-seed obfuscation
    // bugs that don't trip on the canonical seed (e.g. probabilistic
    // rewrites that fire only ~30% of the time).
    let seeds = multi_seeds(source_path);
    for seed in &seeds {
        let r = obfuscate(&src, Options { seed: Some(*seed) })
            .map_err(|e| format!("obfuscate seed={}: {e}", hex(seed)))?;
        let out = run_luau_with_source(&r.output)
            .map_err(|e| format!("run seed={}: {e}", hex(seed)))?;
        if plain != out {
            return Err(format!(
                "output mismatch under seed {}:\n  plain: {plain:?}\n  obfus: {out:?}",
                hex(seed)
            ));
        }
    }
    Ok(())
}

/// 8 deterministic, per-program seeds derived from the file path. Stable across
/// runs (so failures are reproducible) but distinct per program (so a single
/// hardcoded seed doesn't mask per-seed bugs).
fn multi_seeds(source_path: &Path) -> Vec<[u8; 32]> {
    use std::hash::{Hash, Hasher};
    let file_name = source_path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut out = Vec::with_capacity(8);
    // Always include the canonical seed first.
    out.push([0xABu8; 32]);
    for i in 0u8..7 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        file_name.hash(&mut h);
        i.hash(&mut h);
        let mut seed = [0u8; 32];
        // Fill the seed by repeatedly hashing.
        let mut h_state = h.finish();
        for chunk in seed.chunks_mut(8) {
            chunk.copy_from_slice(&h_state.to_le_bytes());
            // Re-mix.
            let mut h2 = std::collections::hash_map::DefaultHasher::new();
            h_state.hash(&mut h2);
            h_state = h2.finish();
        }
        out.push(seed);
    }
    out
}

fn hex(b: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for byte in b {
        s.push_str(&format!("{:02x}", byte));
    }
    s
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
