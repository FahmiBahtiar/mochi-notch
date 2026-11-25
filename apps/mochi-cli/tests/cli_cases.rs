#[test]
fn recorded_cases() {
    trycmd::TestCases::new()
        .register_bin(
            "mochi-cli",
            std::path::Path::new(env!("CARGO_BIN_EXE_mochi-cli")),
        )
        .case("tests/cases/*.toml")
        .skip("tests/cases/skill-show.toml");
}

#[test]
fn skill_show_exact_bytes() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_mochi-cli"))
        .arg("skill")
        .output()
        .expect("run mochi-cli skill");
    assert!(out.status.success());
    let expected = std::fs::read("tests/cases/skill-show.stdout").expect("read skill-show.stdout");
    assert_eq!(out.stdout, expected);
    assert!(out.stderr.is_empty());
}
