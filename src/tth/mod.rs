//! Time/Temp/Humidity import: lê arquivo texto com linhas TEMP= / HUM= / UNIT=.
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct TthReading {
    pub temperature_c: Option<i32>,
    pub humidity_pct: Option<u32>,
    pub unit: Option<String>,
}

pub fn import_tth_file(path: &Path) -> anyhow::Result<TthReading> {
    let text = std::fs::read_to_string(path)?;
    let mut out = TthReading::default();
    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("TEMP=").or_else(|| line.strip_prefix("temp=")) {
            let v = v.trim();
            if let Ok(f) = v.parse::<f32>() {
                out.temperature_c = Some(f.round() as i32);
            } else if let Ok(i) = v.parse::<i32>() {
                out.temperature_c = Some(i);
            }
        } else if let Some(v) = line.strip_prefix("HUM=").or_else(|| line.strip_prefix("humidity=")) {
            if let Ok(u) = v.trim().parse::<u32>() {
                out.humidity_pct = Some(u.min(100));
            }
        } else if let Some(v) = line.strip_prefix("UNIT=").or_else(|| line.strip_prefix("unit=")) {
            out.unit = Some(v.trim().to_string());
        }
    }
    Ok(out)
}

/// Converte °F → °C se unit for F.
pub fn to_celsius(temp: i32, unit: &str) -> i32 {
    if unit.eq_ignore_ascii_case("F") || unit.eq_ignore_ascii_case("fahrenheit") {
        (((temp - 32) as f32) * 5.0 / 9.0).round() as i32
    } else {
        temp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_file() {
        let p = std::env::temp_dir().join("ol_tth.txt");
        std::fs::write(&p, "TEMP=72\nUNIT=F\nHUM=55\n").unwrap();
        let r = import_tth_file(&p).unwrap();
        assert_eq!(r.humidity_pct, Some(55));
        let c = to_celsius(r.temperature_c.unwrap(), r.unit.as_deref().unwrap());
        assert!((c - 22).abs() <= 1);
    }
}
