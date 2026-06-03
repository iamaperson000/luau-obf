use assert_cmd::Command;
use std::fs;

#[test]
fn cli_obfuscates_simple_program() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.luau");
    let output = dir.path().join("out.luau");
    fs::write(&input, "print(1)").unwrap();

    Command::cargo_bin("luau-obf")
        .unwrap()
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .arg("--quiet")
        .assert()
        .success();

    let out = fs::read_to_string(&output).unwrap();
    // After Plan 9 mangling, internal names are opaque. Just confirm the
    // output is a non-trivial Luau program with a top-level return.
    assert!(out.contains("return"));
    assert!(out.len() > 200);
}

#[test]
fn cli_exits_1_on_parse_error() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.luau");
    let output = dir.path().join("out.luau");
    fs::write(&input, "this is not luau {{{").unwrap();

    Command::cargo_bin("luau-obf")
        .unwrap()
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .arg("--quiet")
        .assert()
        .code(1);
}

#[test]
fn cli_accepts_explicit_seed() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.luau");
    let out_a = dir.path().join("a.luau");
    let out_b = dir.path().join("b.luau");
    fs::write(&input, "print(1)").unwrap();

    let seed = "0011223344556677889900112233445566778899001122334455667788990011";

    Command::cargo_bin("luau-obf").unwrap()
        .arg(&input).arg("-o").arg(&out_a).arg("--seed").arg(seed).arg("--quiet")
        .assert().success();
    Command::cargo_bin("luau-obf").unwrap()
        .arg(&input).arg("-o").arg(&out_b).arg("--seed").arg(seed).arg("--quiet")
        .assert().success();

    let a = fs::read_to_string(&out_a).unwrap();
    let b = fs::read_to_string(&out_b).unwrap();
    assert_eq!(a, b, "same seed must produce identical output");
}

#[test]
fn cli_different_seeds_produce_different_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.luau");
    let out_a = dir.path().join("a.luau");
    let out_b = dir.path().join("b.luau");
    fs::write(&input, "print(1) print(2) print(3)").unwrap();

    let seed_a = "1111111111111111111111111111111111111111111111111111111111111111";
    let seed_b = "2222222222222222222222222222222222222222222222222222222222222222";

    Command::cargo_bin("luau-obf").unwrap()
        .arg(&input).arg("-o").arg(&out_a).arg("--seed").arg(seed_a).arg("--quiet")
        .assert().success();
    Command::cargo_bin("luau-obf").unwrap()
        .arg(&input).arg("-o").arg(&out_b).arg("--seed").arg(seed_b).arg("--quiet")
        .assert().success();

    let a = fs::read_to_string(&out_a).unwrap();
    let b = fs::read_to_string(&out_b).unwrap();
    assert_ne!(a, b, "different seeds must produce different output");
}
