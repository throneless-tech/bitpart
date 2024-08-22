use csml_interpreter::data::csml_bot::CsmlBot;
use csml_interpreter::data::csml_flow::CsmlFlow;
use csml_interpreter::data::event::Event as CsmlEvent;
use csml_interpreter::data::{Context as CsmlContext, CsmlResult, MessageData};
use csml_interpreter::validate_bot;
use csml_interpreter::{interpret, load_components};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::{fs, io, path::PathBuf};

pub fn load_flows(path: &str) -> io::Result<Vec<CsmlFlow>> {
    let entries = fs::read_dir(path)?
        .filter_map(|res| match res.ok()?.path() {
            path if path.extension() == Some(OsStr::new("csml")) => {
                let basename = path
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_default();
                Some(CsmlFlow {
                    id: basename.clone(),
                    name: basename,
                    content: fs::read_to_string(path).unwrap_or_default(),
                    commands: vec![],
                })
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    Ok(entries)
}

const DEFAULT_FLOW_NAME: &str = "default";

pub struct CsmlInterpreter {
    bot: CsmlBot,
}

impl CsmlInterpreter {
    pub fn new<S: AsRef<str>>(id: S, name: S, flows: Vec<CsmlFlow>) -> Self {
        let native_components = load_components().unwrap();
        Self {
            bot: CsmlBot::new(
                id.as_ref(),
                name.as_ref(),
                None,
                flows,
                Some(native_components),
                None,
                DEFAULT_FLOW_NAME,
                None,
                None,
                None,
                None,
                None,
            ),
        }
    }

    pub fn validate(self) -> CsmlResult {
        validate_bot(&self.bot)
    }

    pub fn interpret(self, context: CsmlContext, event: CsmlEvent) -> MessageData {
        interpret(self.bot, context, event, None)
    }
}
