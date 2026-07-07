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

////////////////////////////////////////////////////////////////////////////////
/// USE VALID SYNTAX
////////////////////////////////////////////////////////////////////////////////

#[test]
fn use_0() {
    let result = format_message("CSML/basic_test/syntax/use/use_0.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn use_1() {
    let result = format_message("CSML/basic_test/syntax/use/use_1.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn use_2() {
    let result = format_message("CSML/basic_test/syntax/use/use_2.csml".to_owned()).is_ok();

    assert!(result);
}

////////////////////////////////////////////////////////////////////////////////
/// USE INVALID SYNTAX
////////////////////////////////////////////////////////////////////////////////

#[test]
fn use_3() {
    let result = format_message("CSML/basic_test/syntax/use/use_3.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn use_4() {
    let result = format_message("CSML/basic_test/syntax/use/use_4.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn use_5() {
    let result = format_message("CSML/basic_test/syntax/use/use_5.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn use_6() {
    let result = format_message("CSML/basic_test/syntax/use/use_6.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn use_7() {
    let result = format_message("CSML/basic_test/syntax/use/use_7.csml".to_owned()).is_err();

    assert!(result);
}
