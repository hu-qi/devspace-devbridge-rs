use crate::i18n;
use chrono::{Local, TimeZone};

fn is_wide(c: char) -> bool {
    let n = c as u32;
    matches!(
        n,
        0x4E00..=0x9FFF
            | 0x3400..=0x4DBF
            | 0xF900..=0xFAFF
            | 0x3000..=0x303F
            | 0x3040..=0x309F
            | 0x30A0..=0x30FF
            | 0xAC00..=0xD7AF
            | 0xFF01..=0xFF60
            | 0xFE30..=0xFE4F
    )
}

pub fn display_width(value: &str) -> usize {
    value.chars().map(|c| if is_wide(c) { 2 } else { 1 }).sum()
}

pub fn pad_right(value: &str, width: usize) -> String {
    let current = display_width(value);
    if current >= width {
        value.to_owned()
    } else {
        format!("{value}{}", " ".repeat(width - current))
    }
}

pub fn kv(rows: &[(&str, String)]) {
    let width = rows
        .iter()
        .map(|(key, _)| display_width(key))
        .max()
        .unwrap_or(0);
    for (key, value) in rows {
        let label = format!("{key}:");
        print!("{}{}", pad_right(&label, width + 5), value);
        println!();
    }
}

pub fn table(headers: &[&str], rows: &[Vec<String>]) {
    let mut widths = headers.iter().map(|h| display_width(h)).collect::<Vec<_>>();
    for row in rows {
        for (idx, cell) in row.iter().enumerate() {
            if let Some(width) = widths.get_mut(idx) {
                *width = (*width).max(display_width(cell));
            }
        }
    }
    for (idx, header) in headers.iter().enumerate() {
        print!("{}", pad_right(header, widths[idx]));
        if idx + 1 < headers.len() {
            print!("  ");
        }
    }
    println!();
    for row in rows {
        for (idx, cell) in row.iter().enumerate() {
            if idx < widths.len() {
                print!("{}", pad_right(cell, widths[idx]));
            }
            if idx + 1 < row.len() {
                print!("  ");
            }
        }
        println!();
    }
}

pub fn tunnel_expiration(hours: i64) -> String {
    if hours >= 24 {
        format!("{} {}", hours / 24, i18n::days())
    } else {
        format!("{hours} {}", i18n::hours())
    }
}

pub fn tunnel_remaining(expire_at: i64) -> String {
    let now = chrono::Utc::now().timestamp();
    let remaining = expire_at - now;
    if remaining <= 0 {
        return i18n::expired().to_owned();
    }
    let hours = remaining as f64 / 3600.0;
    if hours >= 24.0 {
        remaining_value(hours / 24.0, i18n::days())
    } else {
        remaining_value(hours, i18n::hours())
    }
}

fn remaining_value(value: f64, unit: &str) -> String {
    let rounded = (value * 10.0).ceil() / 10.0;
    let value = if rounded.fract() == 0.0 {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    };
    format!("{value} {unit}")
}

pub fn bytes(value: i64) -> String {
    const UNITS: [&str; 7] = ["B", "KB", "MB", "GB", "TB", "PB", "EB"];
    let mut number = value as f64;
    let mut index = 0usize;
    while number >= 1024.0 && index < UNITS.len() - 1 {
        number /= 1024.0;
        index += 1;
    }
    number = (number * 10.0).round() / 10.0;
    let rendered = if number.fract() == 0.0 {
        format!("{number:.0}")
    } else {
        format!("{number:.1}")
    };
    format!("{rendered} {}", UNITS[index])
}

pub fn time(timestamp: i64) -> String {
    if timestamp == 0 {
        return "-".into();
    }
    Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|value| value.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "-".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_legacy_units() {
        assert_eq!(bytes(1024), "1 KB");
        assert_eq!(bytes(1536), "1.5 KB");
        assert_eq!(display_width("A中"), 3);
    }
}
