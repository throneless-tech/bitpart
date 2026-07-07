use crate::data::{
    ast::Interval,
    error_info::ErrorInfo,
    position::Position,
    primitive::PrimitiveType,
    primitive::{Data, PrimitiveInt, PrimitiveNull, PrimitiveObject},
    Literal,
};
use crate::error_format::*;
use chrono::{DateTime, NaiveDate, NaiveDateTime, SecondsFormat, TimeZone, Utc};
use std::collections::HashMap;

////////////////////////////////////////////////////////////////////////////////
// PRIVATE FUNCTIONS
////////////////////////////////////////////////////////////////////////////////

fn get_date_string(
    args: &HashMap<String, Literal>,
    index: usize,
    data: &mut Data,
    interval: Interval,
    error: &str,
) -> Result<String, ErrorInfo> {
    match args.get(&format!("arg{}", index)) {
        Some(literal) if literal.primitive.get_type() == PrimitiveType::PrimitiveString => {
            let value = Literal::get_value::<String>(
                &literal.primitive,
                &data.context.flow,
                literal.interval,
                error.to_string(),
            )?;

            Ok(value.to_owned())
        }
        _ => Err(gen_error_info(
            Position::new(interval, &data.context.flow),
            error.to_string(),
        )),
    }
}

////////////////////////////////////////////////////////////////////////////////
// PUBLIC FUNCTIONS
////////////////////////////////////////////////////////////////////////////////

pub fn get_date(args: &HashMap<String, Literal>) -> [i64; 7] {
    let mut date: [i64; 7] = [0; 7];

    // set default month, day, and hour to 1 year does not need to have a default
    // value because set_date_at expect at least the year value as parameter
    date[1] = 1; // month
    date[2] = 1; // day
    date[3] = 1; // hour

    // Iterate over `date` slots so extra positional args cannot index past the array.
    for (index, slot) in date.iter_mut().enumerate() {
        if let Some(lit) = args.get(&format!("arg{}", index)) {
            if lit.primitive.get_type() == PrimitiveType::PrimitiveInt {
                *slot = serde_json::from_str(&lit.primitive.to_string()).unwrap();
            }
        }
    }

    date
}

pub fn parse_rfc3339(
    args: &HashMap<String, Literal>,
    data: &mut Data,
    interval: Interval,
) -> Result<Literal, ErrorInfo> {
    let usage = "invalid value, 'parse(String)' expect a valid RFC 3339 and ISO 8601 date and time string such as '1996-12-19T00:00:00Z'";

    let date_str = get_date_string(args, 0, data, interval, usage)?;

    // autocomplete format with default values
    let date_str = match date_str.len() {
        4 => format!(
            "{}-{a1}-{a1}T{a2}:{a2}:{a2}Z",
            date_str,
            a1 = "01",
            a2 = "00"
        ),
        7 => format!("{}-{a1}T{a2}:{a2}:{a2}Z", date_str, a1 = "01", a2 = "00"),
        10 => format!("{}T{a2}:{a2}:{a2}Z", date_str, a2 = "00"),
        13 => format!("{}:{a2}:{a2}Z", date_str, a2 = "00"),
        16 => format!("{}:{a2}Z", date_str, a2 = "00"),
        19 => format!("{}Z", date_str),
        _ => date_str.to_owned(),
    };

    let date = match DateTime::parse_from_rfc3339(&date_str) {
        Ok(date) => date,
        Err(_) => {
            return Err(gen_error_info(
                Position::new(interval, &data.context.flow),
                usage.to_string(),
            ))
        }
    };

    let mut object = HashMap::new();

    object.insert(
        "milliseconds".to_owned(),
        PrimitiveInt::get_literal(date.to_utc().timestamp_millis(), interval),
    );

    let offset: i32 = date.timezone().local_minus_utc();
    if offset != 0 {
        object.insert(
            "offset".to_owned(),
            PrimitiveInt::get_literal(offset as i64, interval),
        );
    }

    let mut lit = PrimitiveObject::get_literal(&object, interval);
    lit.set_content_type("time");

    Ok(lit)
}

pub fn pasre_from_str(
    args: &HashMap<String, Literal>,
    data: &mut Data,
    interval: Interval,
) -> Result<Literal, ErrorInfo> {
    let usage = "invalid value,
    expect date and a specified format to parse the date example:
    parse(\"1983 08 13 12:09:14.274\", \"%Y %m %d %H:%M:%S%.3f\")";

    let date_str = get_date_string(args, 0, data, interval, usage)?;

    let format_str = get_date_string(args, 1, data, interval, usage)?;

    let date_millis = if let Ok(date) = DateTime::parse_from_str(&date_str, &format_str) {
        date.to_utc().timestamp_millis()
    } else if let Ok(naive_datetime) = NaiveDateTime::parse_from_str(&date_str, &format_str) {
        let date = DateTime::<Utc>::from_naive_utc_and_offset(naive_datetime, Utc);
        date.to_utc().timestamp_millis()
    } else if let Ok(naive_date) = NaiveDate::parse_from_str(&date_str, &format_str) {
        let naive_datetime: NaiveDateTime = naive_date.and_hms_opt(0, 0, 0).unwrap();
        let date = DateTime::<Utc>::from_naive_utc_and_offset(naive_datetime, Utc);
        date.to_utc().timestamp_millis()
    } else {
        return Err(gen_error_info(
            Position::new(interval, &data.context.flow),
            usage.to_string(),
        ));
    };

    let mut object = HashMap::new();
    object.insert(
        "milliseconds".to_owned(),
        PrimitiveInt::get_literal(date_millis, interval),
    );

    let mut lit = PrimitiveObject::get_literal(&object, interval);
    lit.set_content_type("time");

    Ok(lit)
}

pub fn format_date<Tz>(
    args: &HashMap<String, Literal>,
    date: DateTime<Tz>,
    data: &mut Data,
    interval: Interval,
    use_z: bool,
) -> Result<String, ErrorInfo>
where
    Tz: TimeZone,
    Tz::Offset: core::fmt::Display,
{
    match args.len() {
        0 => Ok(date.to_rfc3339_opts(SecondsFormat::Millis, use_z)),
        _ => {
            let format_lit = match args.get("arg0") {
                Some(res) => res.to_owned(),
                _ => PrimitiveNull::get_literal(Interval::default()),
            };

            let format = Literal::get_value::<String>(
                &format_lit.primitive,
                &data.context.flow,
                interval,
                "format parameter must be of type string".to_string(),
            )?;

            Ok(date.format(format).to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int_arg(map: &mut HashMap<String, Literal>, key: &str, value: i64) {
        map.insert(
            key.to_owned(),
            PrimitiveInt::get_literal(value, Interval::default()),
        );
    }

    #[test]
    fn get_date_ignores_extra_positional_args_without_panicking() {
        let mut args = HashMap::new();
        for i in 0..10 {
            int_arg(&mut args, &format!("arg{}", i), (i as i64) + 1);
        }
        let date = get_date(&args);
        // first 7 slots filled from arg0..arg6 (1..=7); extras (arg7..arg9) ignored.
        assert_eq!(date, [1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn get_date_defaults_month_day_hour_when_only_year_given() {
        let mut args = HashMap::new();
        int_arg(&mut args, "arg0", 2026);
        let date = get_date(&args);
        // year set; month/day/hour default to 1; rest 0.
        assert_eq!(date, [2026, 1, 1, 1, 0, 0, 0]);
    }
}
