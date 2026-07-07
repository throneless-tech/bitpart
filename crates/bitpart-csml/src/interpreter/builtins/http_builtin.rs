use crate::data::error_info::ErrorInfo;
use crate::data::position::Position;
use crate::data::primitive::{PrimitiveInt, PrimitiveObject, PrimitiveString, PrimitiveType};
use crate::data::{ast::Interval, csml_logs::*, ArgsType, Literal};
use crate::error_format::*;
use std::collections::HashMap;
use std::env;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use reqwest::{Client, Method, StatusCode};

////////////////////////////////////////////////////////////////////////////////
// PRIVATE FUNCTIONS
////////////////////////////////////////////////////////////////////////////////

fn get_value<'lifetime, T: 'static>(
    key: &str,
    object: &'lifetime HashMap<String, Literal>,
    flow_name: &str,
    interval: Interval,
    error: &'static str,
) -> Result<&'lifetime T, ErrorInfo> {
    if let Some(literal) = object.get(key) {
        Literal::get_value::<T>(
            &literal.primitive,
            flow_name,
            interval,
            format!("'{}' {}", key, error),
        )
    } else {
        Err(gen_error_info(
            Position::new(interval, flow_name),
            format!("'{}' {}", key, error),
        ))
    }
}

fn set_http_error_info(
    response_info: &HashMap<String, Literal>,
    error_message: String,
    flow_name: &str,
    interval: Interval,
) -> ErrorInfo {
    let mut error = gen_error_info(Position::new(interval, flow_name), error_message);
    error.add_info_block(response_info.clone());

    error
}

fn get_request_info(
    status: StatusCode,
    headers: &HeaderMap,
    interval: Interval,
) -> HashMap<String, Literal> {
    let mut response_info = HashMap::new();

    let status = PrimitiveInt::get_literal(status.as_u16() as i64, interval);
    response_info.insert("status".to_owned(), status);

    let headers = headers
        .iter()
        .fold(HashMap::new(), |mut acc, (name, value)| {
            if let Ok(value) = value.to_str() {
                let value = PrimitiveString::get_literal(value, interval);
                acc.insert(name.as_str().to_owned(), value);
            }
            acc
        });

    response_info.insert(
        "headers".to_owned(),
        PrimitiveObject::get_literal(&headers, interval),
    );

    response_info
}

pub fn get_ssl_state(object: &HashMap<String, Literal>) -> bool {
    match object.get("disable_ssl_verify") {
        Some(val) if val.primitive.get_type() == PrimitiveType::PrimitiveBoolean => {
            val.primitive.as_bool()
        }
        _ => false,
    }
}

////////////////////////////////////////////////////////////////////////////////
// PUBLIC FUNCTIONS
////////////////////////////////////////////////////////////////////////////////

pub fn get_url(
    object: &HashMap<String, Literal>,
    flow_name: &str,
    interval: Interval,
) -> Result<String, ErrorInfo> {
    let url = &mut get_value::<String>("url", object, flow_name, interval, ERROR_HTTP_GET_VALUE)?
        .to_owned();

    if object.get("query").is_some() {
        let query = get_value::<HashMap<String, Literal>>(
            "query",
            object,
            flow_name,
            interval,
            ERROR_HTTP_GET_VALUE,
        )?;

        let length = query.len();
        if length > 0 {
            url.push('?');

            for (index, key) in query.keys().enumerate() {
                let value = match query.get(key) {
                    Some(val) => val.primitive.to_string(),
                    None => {
                        return Err(gen_error_info(
                            Position::new(interval, flow_name),
                            format!("'{}' {}", key, ERROR_HTTP_GET_VALUE),
                        ))
                    }
                };

                url.push_str(key);
                url.push('=');
                url.push_str(&value);

                if index + 1 < length {
                    url.push('&');
                }
            }
        }
    }

    Ok(url.to_owned())
}

fn get_no_certificate_verifier_agent() -> Result<Client, reqwest::Error> {
    Client::builder()
        .danger_accept_invalid_certs(true)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
}

fn parse_method(method: &str, flow_name: &str, interval: Interval) -> Result<Method, ErrorInfo> {
    match method {
        "delete" => Ok(Method::DELETE),
        "put" => Ok(Method::PUT),
        "patch" => Ok(Method::PATCH),
        "post" => Ok(Method::POST),
        "get" => Ok(Method::GET),
        _ => Err(gen_error_info(
            Position::new(interval, flow_name),
            ERROR_HTTP_UNKNOWN_METHOD.to_string(),
        )),
    }
}

// TLS verification is disabled when EITHER the per-call `disable_ssl_verify` flag is set,
// OR the DISABLE_SSL_VERIFY env var is present and parses as `true`.
fn get_http_client(is_ssl_disable: bool) -> Result<Client, reqwest::Error> {
    let env_disable = env::var("DISABLE_SSL_VERIFY")
        .ok()
        .and_then(|value| value.parse::<bool>().ok())
        .unwrap_or(false);

    if is_ssl_disable || env_disable {
        return get_no_certificate_verifier_agent();
    }

    Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
}

fn get_http_request(
    method: &str,
    url: &str,
    flow_name: &str,
    interval: Interval,
    is_ssl_disable: bool,
) -> Result<reqwest::RequestBuilder, ErrorInfo> {
    let method = parse_method(method, flow_name, interval)?;

    let client = get_http_client(is_ssl_disable)
        .map_err(|err| gen_error_info(Position::new(interval, flow_name), err.to_string()))?;

    Ok(client.request(method, url))
}

pub async fn http_request(
    object: &HashMap<String, Literal>,
    method: &str,
    flow_name: &str,
    interval: Interval,
    is_app_call: bool,
) -> Result<(serde_json::Value, HashMap<String, Literal>), ErrorInfo> {
    let url = get_url(object, flow_name, interval)?;
    let is_ssl_disable = get_ssl_state(object);

    let header = get_value::<HashMap<String, Literal>>(
        "header",
        object,
        flow_name,
        interval,
        ERROR_HTTP_GET_VALUE,
    )?;

    let mut request = get_http_request(method, &url, flow_name, interval, is_ssl_disable)?;

    let mut has_content_type = false;
    let mut header_map = HeaderMap::new();
    for key in header.keys() {
        let value = match header.get(key) {
            Some(val) => val.primitive.to_string(),
            None => {
                return Err(gen_error_info(
                    Position::new(interval, flow_name),
                    format!("'{}' {}", key, ERROR_HTTP_GET_VALUE),
                ))
            }
        };

        let header_name = HeaderName::from_bytes(key.as_bytes())
            .map_err(|err| gen_error_info(Position::new(interval, flow_name), err.to_string()))?;
        let header_value = HeaderValue::from_str(&value)
            .map_err(|err| gen_error_info(Position::new(interval, flow_name), err.to_string()))?;

        if header_name == CONTENT_TYPE {
            has_content_type = true;
        }

        header_map.insert(header_name, header_value);
    }
    request = request.headers(header_map);

    if let Some(body) = object.get("body") {
        let json = body.primitive.to_json();
        let bytes = serde_json::to_vec(&json).unwrap_or_default();
        if !has_content_type {
            request = request.header(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        }
        request = request.body(bytes);
    }

    csml_logger(
        CsmlLog::new(
            None,
            Some(flow_name.to_string()),
            Some(interval.start_line),
            "Make Http call".to_string(),
        ),
        LogLvl::Info,
    );
    csml_logger(
        CsmlLog::new(
            None,
            Some(flow_name.to_string()),
            Some(interval.start_line),
            format!("Make Http call request info: method={} url={}", method, url),
        ),
        LogLvl::Debug,
    );

    let result: Result<(StatusCode, HeaderMap, String, String), reqwest::Error> = async move {
        let response = request.send().await?;
        let status = response.status();
        let headers = response.headers().clone();
        let final_url = response.url().to_string();
        let body = response.text().await?;
        Ok((status, headers, body, final_url))
    }
    .await;

    match result {
        Ok((status, headers, body, final_url)) => {
            let response_info = get_request_info(status, &headers, interval);

            if status.as_u16() >= 400 {
                let error_message = match is_app_call {
                    true => format!("Apps service: status code {}", status.as_u16()),
                    false if final_url != url => format!(
                        "{}: status code {} (redirected from {})",
                        final_url,
                        status.as_u16(),
                        url
                    ),
                    false => format!("{}: status code {}", final_url, status.as_u16()),
                };

                csml_logger(
                    CsmlLog::new(
                        None,
                        Some(flow_name.to_string()),
                        Some(interval.start_line),
                        format!("Http call failed: {:?}", error_message),
                    ),
                    LogLvl::Error,
                );

                let mut error =
                    set_http_error_info(&response_info, error_message, flow_name, interval);
                error.add_info("body", PrimitiveString::get_literal(&body, interval));

                return Err(error);
            }

            match serde_json::from_str::<serde_json::Value>(&body) {
                Ok(json_value) => Ok((json_value, response_info)),
                Err(_) => Ok((serde_json::json!(body), response_info)),
            }
        }
        Err(err) => {
            let error_message = match is_app_call {
                true => "Apps service: error".to_string(),
                false => err.to_string(),
            };

            csml_logger(
                CsmlLog::new(
                    None,
                    Some(flow_name.to_string()),
                    Some(interval.start_line),
                    format!("Http call failed: {:?}", error_message),
                ),
                LogLvl::Error,
            );

            Err(gen_error_info(
                Position::new(interval, flow_name),
                error_message,
            ))
        }
    }
}

pub fn http(args: ArgsType, flow_name: &str, interval: Interval) -> Result<Literal, ErrorInfo> {
    let mut http: HashMap<String, Literal> = HashMap::new();
    let mut header = HashMap::new();

    match args.get("url", 0) {
        Some(literal) if literal.primitive.get_type() == PrimitiveType::PrimitiveString => {
            header.insert(
                "Content-Type".to_owned(),
                PrimitiveString::get_literal("application/json", interval),
            );
            header.insert(
                "Accept".to_owned(),
                PrimitiveString::get_literal("application/json,text/*", interval),
            );
            header.insert(
                "User-Agent".to_owned(),
                PrimitiveString::get_literal("csml/v1", interval),
            );

            http.insert("url".to_owned(), literal.to_owned());
            http.insert(
                "method".to_owned(),
                PrimitiveString::get_literal("get", interval),
            );

            let lit_header = PrimitiveObject::get_literal(&header, interval);
            http.insert("header".to_owned(), lit_header);

            args.populate(
                &mut http,
                &["url", "header", "query", "body"],
                flow_name,
                interval,
            )?;

            let mut result = PrimitiveObject::get_literal(&http, interval);

            result.set_content_type("http");

            Ok(result)
        }
        _ => Err(gen_error_info(
            Position::new(interval, flow_name),
            ERROR_HTTP.to_owned(),
        )),
    }
}
