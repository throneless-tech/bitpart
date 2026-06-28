mod support;

use bitpart_csml::data::ast::Flow;
use bitpart_csml::error_format::ErrorInfo;
use bitpart_csml::parser::parse_flow;

use support::tools::read_file;

#[allow(clippy::result_large_err)] // ErrorInfo is intentionally large (see lib.rs)
fn format_message(filepath: String) -> Result<Flow, ErrorInfo> {
    let text = read_file(filepath).unwrap();

    parse_flow(&text, "Test")
}

#[test]
fn do_0() {
    let result = format_message("CSML/basic_test/syntax/do/do_0.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn do_1() {
    let result = format_message("CSML/basic_test/syntax/do/do_1.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn do_2() {
    let result = format_message("CSML/basic_test/syntax/do/do_2.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn do_3() {
    let result = format_message("CSML/basic_test/syntax/do/do_3.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn do_4() {
    let result = format_message("CSML/basic_test/syntax/do/do_4.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn do_5() {
    let result = format_message("CSML/basic_test/syntax/do/do_5.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn do_6() {
    let result = format_message("CSML/basic_test/syntax/do/do_6.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn do_7() {
    let result = format_message("CSML/basic_test/syntax/do/do_7.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn do_8() {
    let result = format_message("CSML/basic_test/syntax/do/do_8.csml".to_owned()).is_err();

    assert!(result);
}
