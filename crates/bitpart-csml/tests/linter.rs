// mod support;

// use bitpart_csml::data::ast::Flow;
// use bitpart_csml::error_format::ErrorInfo;
// use bitpart_csml::parse_file;

// use support::tools::read_file;

// fn format_message(filepath: String) -> Result<Flow, ErrorInfo> {
//     let text = read_file(filepath).unwrap();

//     parse_file(&text)
// }

// ////////////////////////////////////////////////////////////////////////////////
// /// FOREACH VALID SYNTAX
// ////////////////////////////////////////////////////////////////////////////////

// #[test]
// fn duplicate_step() {
//     let result = match format_message("CSML/basic_test/linter/duplicate_step.csml".to_owned()) {
//         Ok(_) => false,
//         Err(_) => true,
//     };

//     assert!(result);
// }

// #[test]
// fn missing_start() {
//     let result = match format_message("CSML/basic_test/linter/missing_start.csml".to_owned()) {
//         Ok(_) => false,
//         Err(_) => true,
//     };

//     assert!(result);
// }

// #[test]
// fn wrong_goto_step() {
//     let result = match format_message("CSML/basic_test/linter/wrong_goto_step.csml".to_owned()) {
//         Ok(_) => false,
//         Err(_) => true,
//     };

//     assert!(result);
// }

// #[test]
// fn valid_flow() {
//     let result = match format_message("CSML/basic_test/linter/valid_flow.csml".to_owned()) {
//         Ok(_) => true,
//         Err(_) => false,
//     };

//     assert!(result);
// }
