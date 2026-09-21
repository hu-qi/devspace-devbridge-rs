pub fn bytes(v: i64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut n = v.max(0) as f64;
    let mut i = 0usize;
    while n >= 1024.0 && i < UNITS.len() - 1 {
        n /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{} {}", n as i64, UNITS[i]) } else { format!("{n:.1} {}", UNITS[i]) }
}

pub fn kv(rows: &[(&str, String)]) {
    let width = rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (k, v) in rows {
        println!("{k:width$}  {v}");
    }
}
