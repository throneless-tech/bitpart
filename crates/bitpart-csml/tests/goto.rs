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
fn goto_0() {
    let result = format_message("CSML/basic_test/syntax/goto/goto_0.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn goto_1() {
    let result = format_message("CSML/basic_test/syntax/goto/goto_1.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn goto_2() {
    let result = format_message("CSML/basic_test/syntax/goto/goto_2.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn goto_3() {
    let result = format_message("CSML/basic_test/syntax/goto/goto_3.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn goto_4() {
    let result = format_message("CSML/basic_test/syntax/goto/goto_4.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn goto_5() {
    let result = format_message("CSML/basic_test/syntax/goto/goto_5.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn goto_6() {
    let result = format_message("CSML/basic_test/syntax/goto/goto_6.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn goto_7() {
    let result = format_message("CSML/basic_test/syntax/goto/goto_7.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn goto_8() {
    let result = format_message("CSML/basic_test/syntax/goto/goto_8.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn goto_9() {
    let result = format_message("CSML/basic_test/syntax/goto/goto_9.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn goto_10() {
    let result = format_message("CSML/basic_test/syntax/goto/goto_10.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn goto_11() {
    let result = format_message("CSML/basic_test/syntax/goto/goto_11.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn goto_12() {
    let result = format_message("CSML/basic_test/syntax/goto/goto_12.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn goto_13() {
    let result = format_message("CSML/basic_test/syntax/goto/goto_13.csml".to_owned()).is_err();

    assert!(result);
}

#[test]
fn goto_14() {
    let result = format_message("CSML/basic_test/syntax/goto/goto_14.csml".to_owned()).is_err();

    assert!(result);
}
