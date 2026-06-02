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
    assert!(out.contains("vm_call"));
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
