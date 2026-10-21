use rbx_loren::specs::{loren_unit_place, run_specs};

#[test]
fn loren_unit_specs_pass() {
    let dom = loren_unit_place().expect("unit place");
    let report =
        run_specs(dom, &["ServerStorage", "LorenTest", "Specs", "Unit"], None).expect("spec run");
    println!("{}", report.report);
    println!(
        "passed {} failed {} skipped {}",
        report.passed, report.failed, report.skipped
    );
    assert_eq!(report.failed, 0, "{:#?}", report.failures);
}
