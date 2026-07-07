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
/// SAY VALID SYNTAX
////////////////////////////////////////////////////////////////////////////////

#[test]
fn say_0() {
    let result = format_message("CSML/basic_test/syntax/say/say_0.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn say_1() {
    let result = format_message("CSML/basic_test/syntax/say/say_1.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn say_2() {
    let result = format_message("CSML/basic_test/syntax/say/say_2.csml".to_owned()).is_ok();

    assert!(result);
}

////////////////////////////////////////////////////////////////////////////////
/// AS INVALID SYNTAX
////////////////////////////////////////////////////////////////////////////////

#[test]
fn say_3() {
    let result = format_message("CSML/basic_test/syntax/say/say_3.csml".to_owned()).is_err();

    assert!(result);
}
