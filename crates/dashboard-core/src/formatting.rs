use core::fmt::{self, Write};

use heapless::String;

pub type DisplayString = String<48>;

fn formatted(arguments: fmt::Arguments<'_>) -> DisplayString {
    let mut output = DisplayString::new();
    if output.write_fmt(arguments).is_err() {
        output.clear();
        let _ = output.push_str("??");
    }
    output
}

pub fn watts(value: f64) -> DisplayString {
    if value < 1000.0 {
        formatted(format_args!("{value:.0} W"))
    } else {
        formatted(format_args!("{:.1} kW", value / 1000.0))
    }
}

pub fn watts_value(value: f64) -> DisplayString {
    if value < 1000.0 {
        formatted(format_args!("{value:.0}"))
    } else {
        formatted(format_args!("{:.1}", value / 1000.0))
    }
}

pub fn kilowatts_value(value: f64) -> DisplayString {
    formatted(format_args!("{:.1}", value / 1000.0))
}

pub fn kilowatt_hours_value(value: f64) -> DisplayString {
    formatted(format_args!("{value:.1}"))
}

pub fn watts_in_out(value_in: f64, value_out: f64) -> DisplayString {
    let direction = if value_in > 0.0 {
        "↓"
    } else if value_out > 0.0 {
        "↑"
    } else {
        ""
    };
    let value = if value_out > 0.0 { value_out } else { value_in };

    if value < 1000.0 {
        formatted(format_args!("{direction} {value:.0}"))
    } else {
        formatted(format_args!("{direction} {:.1}", value / 1000.0))
    }
}

pub fn kilowatt_hours_in_out(value_in: f64, value_out: f64) -> DisplayString {
    formatted(format_args!("↓{value_in:.1} ↑{value_out:.1}"))
}

pub fn power_with_unit(value: f64) -> DisplayString {
    if value < 1000.0 {
        formatted(format_args!("{value:.0}W"))
    } else {
        formatted(format_args!("{:.1}kW", value / 1000.0))
    }
}

pub fn battery_power(value_in: f64, value_out: f64) -> DisplayString {
    let direction = if value_in > 0.0 {
        "↓ "
    } else if value_out > 0.0 {
        "↑ "
    } else {
        " "
    };
    let mut output = formatted(format_args!("{direction}"));
    let value = if value_out > 0.0 { value_out } else { value_in };
    let _ = output.push_str(power_with_unit(value).as_str());
    output
}

pub fn battery_energy(value: f64) -> DisplayString {
    formatted(format_args!("{value}kW"))
}

pub fn percentage(value: f64) -> DisplayString {
    formatted(format_args!("{value:.0}%"))
}

pub fn temperature(value: f64) -> DisplayString {
    if value < -90.0 {
        DisplayString::try_from("??°").unwrap_or_default()
    } else {
        formatted(format_args!("{value}°"))
    }
}

pub fn humidity(value: f64) -> DisplayString {
    formatted(format_args!("{value}%"))
}

pub fn cop(value: f64) -> DisplayString {
    formatted(format_args!("COP {value:.1}"))
}

pub fn date(year: i32, month: u8, day: u8, weekday: u8) -> DisplayString {
    let _ = year;
    const WEEKDAYS: [&str; 7] = ["zo", "ma", "di", "wo", "do", "vr", "za"];
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mrt", "apr", "mei", "jun", "jul", "aug", "sep", "okt", "nov", "dec",
    ];
    let weekday = WEEKDAYS.get(usize::from(weekday)).copied().unwrap_or("??");
    let month = month
        .checked_sub(1)
        .and_then(|index| MONTHS.get(usize::from(index)))
        .copied()
        .unwrap_or("???");
    formatted(format_args!("{weekday} {day:2} {month}"))
}

pub fn time(hour: u8, minute: u8) -> DisplayString {
    formatted(format_args!("{hour:02}:{minute:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_formatting_boundaries_are_preserved() {
        assert_eq!(watts_value(999.0).as_str(), "999");
        assert_eq!(watts_value(1000.0).as_str(), "1.0");
        assert_eq!(watts_in_out(0.0, 3000.0).as_str(), "↑ 3.0");
        assert_eq!(watts_in_out(100.0, 3000.0).as_str(), "↓ 3.0");
        assert_eq!(battery_power(4400.0, 0.0).as_str(), "↓ 4.4kW");
    }

    #[test]
    fn formats_dutch_status_bar() {
        assert_eq!(date(2026, 9, 9, 3).as_str(), "wo  9 sep");
        assert_eq!(time(7, 5).as_str(), "07:05");
    }
}
