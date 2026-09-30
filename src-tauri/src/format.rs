/// Compact duration: `45s`, `12m`, `2h 10m`, `3d 4h`.
pub fn duration_short(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, (secs % 86_400) / 3600, (secs % 3600) / 60);
    match (d, h, m) {
        (0, 0, 0) => format!("{secs}s"),
        (0, 0, m) => format!("{m}m"),
        (0, h, m) => format!("{h}h {m}m"),
        (d, h, _) => format!("{d}d {h}h"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(duration_short(45), "45s");
        assert_eq!(duration_short(12 * 60 + 5), "12m");
        assert_eq!(duration_short(2 * 3600 + 10 * 60), "2h 10m");
        assert_eq!(duration_short(3 * 86_400 + 4 * 3600 + 59), "3d 4h");
    }
}
