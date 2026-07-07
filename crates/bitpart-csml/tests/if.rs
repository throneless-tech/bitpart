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
fn if_0() {
    let result = format_message("CSML/basic_test/syntax/if/if_0.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn if_1() {
    let result = format_message("CSML/basic_test/syntax/if/if_1.csml".to_owned()).is_ok();

    assert!(result);
}

#[test]
fn if_2() {
    let result = format_message("CSML/basic_test/syntax/if/if_2.csml".to_owned()).is_err();

    assert!(result);
}
