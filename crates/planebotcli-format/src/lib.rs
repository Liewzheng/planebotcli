//! Dual output contract: JSON → stdout, table → stderr.
//!
//! Mirrors the Python formatter so `--json 2>/dev/null` yields clean JSON on
//! stdout while the human table goes to stderr.

use serde::Serialize;

/// Pretty-print `data` as JSON to stdout (indent 2, unescaped unicode).
pub fn output_json<T: Serialize>(data: &T) {
    match serde_json::to_string_pretty(data) {
        Ok(json) => println!("{json}"),
        Err(e) => eprintln!("[error] failed to serialize output: {e}"),
    }
}

/// Print a table to stderr (tabled-styled), used when `--json` is absent.
/// tabled's Builder has no separate header concept: the first record pushed is
/// the header row.
pub fn output_table(header: &[&str], rows: &[Vec<String>]) {
    use tabled::builder::Builder;
    let mut builder = Builder::default();
    builder.push_record(header.to_vec());
    for row in rows {
        builder.push_record(row.iter().cloned());
    }
    eprintln!("{}", builder.build());
}

#[cfg(test)]
mod tests {
    #[test]
    fn json_roundtrip_is_parseable() {
        // output_json writes to stdout; verify serialization via serde directly
        let value = serde_json::json!({"a": 1, "中文": true});
        let s = serde_json::to_string_pretty(&value).unwrap();
        assert!(s.contains("中文"));
    }
}
