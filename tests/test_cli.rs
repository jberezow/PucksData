/// The command dispatcher contains large async futures. Exercise the real debug
/// binary so added writer wrappers cannot silently overflow the entry-point stack.
#[test]
fn cli_starts_and_exposes_maintenance_command() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_pucksdata"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("refresh-derived"));
}
