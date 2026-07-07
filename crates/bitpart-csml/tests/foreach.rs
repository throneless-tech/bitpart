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
fn foreach_0() {
    let result = format_message("CSML/basic_test/syntax/foreach/foreach_0.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn foreach_1() {
    let result = format_message("CSML/basic_test/syntax/foreach/foreach_1.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn foreach_2() {
    let result = format_message("CSML/basic_test/syntax/foreach/foreach_2.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn foreach_3() {
    let result = format_message("CSML/basic_test/syntax/foreach/foreach_3.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn foreach_4() {
    let result = format_message("CSML/basic_test/syntax/foreach/foreach_4.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn foreach_5() {
    let result = format_message("CSML/basic_test/syntax/foreach/foreach_5.csml".to_owned()).is_ok();

    assert!(result);
}

////////////////////////////////////////////////////////////////////////////////
/// FOREACH INVALID SYNTAX
////////////////////////////////////////////////////////////////////////////////

#[test]
fn foreach_6() {
    let result =
        format_message("CSML/basic_test/syntax/foreach/foreach_6.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn foreach_7() {
    let result =
        format_message("CSML/basic_test/syntax/foreach/foreach_7.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn foreach_8() {
    let result =
        format_message("CSML/basic_test/syntax/foreach/foreach_8.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn foreach_9() {
    let result =
        format_message("CSML/basic_test/syntax/foreach/foreach_9.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn foreach_10() {
    let result =
        format_message("CSML/basic_test/syntax/foreach/foreach_10.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn foreach_11() {
    let result =
        format_message("CSML/basic_test/syntax/foreach/foreach_11.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn foreach_12() {
    let result =
        format_message("CSML/basic_test/syntax/foreach/foreach_12.csml".to_owned()).is_err();

    assert!(result);
}
