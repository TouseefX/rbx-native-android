use rbx_dom::WeakDom;
use rbx_reflection::ReflectionDatabase;

use super::*;
use crate::runtime::RunState;

const PLAIN: &str = "return function(v) return tostring(v) end";

fn runtime() -> Runtime {
    let mut dom = WeakDom::new();
    dom.new_instance("Workspace", "Workspace", None);
    Runtime::new(dom, ReflectionDatabase::embedded()).unwrap()
}

fn reported(source: &str) -> Result<String, String> {
    let mut runtime = runtime();
    let slot = runtime.start_reported(source, PLAIN);
    for _ in 0..50 {
        if slot.borrow().is_some() {
            break;
        }
        match runtime.resume().state {
            RunState::Pending { wake_in } => std::thread::sleep(wake_in),
            _ => break,
        }
    }
    let answer = slot.borrow().clone();
    answer.expect("the run reported nothing")
}

#[test]
fn bare_expression_yields_its_value() {
    assert_eq!(reported("1 + 2"), Ok("3".to_string()));
}

#[test]
fn statements_fall_back_to_the_plain_source() {
    assert_eq!(reported("local x = 5 return x * 2"), Ok("10".to_string()));
}

#[test]
fn runtime_error_is_reported() {
    let error = reported("error(\"boom\")").unwrap_err();
    assert!(error.contains("boom"), "{error}");
}

#[test]
fn a_run_that_yields_reports_after_resumes() {
    assert_eq!(reported("task.wait(0) return 7"), Ok("7".to_string()));
}

#[test]
fn syntax_error_is_reported_at_once() {
    let mut runtime = runtime();
    let slot = runtime.start_reported("local = ", PLAIN);
    assert!(matches!(&*slot.borrow(), Some(Err(_))));
}

#[test]
fn a_caught_error_still_reaches_the_output() {
    let mut runtime = runtime();
    let _slot = runtime.start_reported("error(\"boom\")", PLAIN);
    let step = runtime.resume();
    assert_eq!(step.output.kinds(), [crate::LineKind::Error]);
    assert!(step.output.lines()[0].contains("boom"));
}

#[test]
fn a_syntax_error_fails_the_run_like_the_command_bar() {
    let mut runtime = runtime();
    let _slot = runtime.start_reported("local = ", PLAIN);
    assert!(matches!(runtime.resume().state, RunState::Failed(_)));
}
