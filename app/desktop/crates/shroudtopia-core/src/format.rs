//! pt-BR words for times and sizes, as the prototype writes them: "hoje, 14:10 – 16:02", "ontem, 23:40", "dom",
//! "17 out", "1h52", "1,8 MB", "145 KB". Times are shown in the PC's offset, passed in (the core never asks the OS).

use chrono::{DateTime, Datelike, FixedOffset, Utc};

const WEEKDAYS: [&str; 7] = ["seg", "ter", "qua", "qui", "sex", "sáb", "dom"];
const MONTHS: [&str; 12] = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];

pub fn hm(t: DateTime<Utc>, off: FixedOffset) -> String {
    t.with_timezone(&off).format("%H:%M").to_string()
}

fn date(t: DateTime<Utc>, off: FixedOffset) -> String {
    let l = t.with_timezone(&off);
    format!("{} {}", l.day(), MONTHS[l.month0() as usize])
}

/// "hoje", "ontem", a weekday within the last week ("dom"), else the date ("17 out").
pub fn day(t: DateTime<Utc>, now: DateTime<Utc>, off: FixedOffset) -> String {
    let (d, n) = (t.with_timezone(&off).date_naive(), now.with_timezone(&off).date_naive());
    match (n - d).num_days() {
        0 => "hoje".into(),
        1 => "ontem".into(),
        2..=6 => WEEKDAYS[t.with_timezone(&off).weekday().num_days_from_monday() as usize].into(),
        _ => date(t, off),
    }
}

/// "ontem, 23:40"
pub fn day_time(t: DateTime<Utc>, now: DateTime<Utc>, off: FixedOffset) -> String {
    format!("{}, {}", day(t, now, off), hm(t, off))
}

/// "hoje, 14:10 – 16:02", or "hoje, desde 21:12" for a session still running.
pub fn range(start: DateTime<Utc>, end: Option<DateTime<Utc>>, now: DateTime<Utc>, off: FixedOffset) -> String {
    match end {
        Some(end) => format!("{}, {} – {}", day(start, now, off), hm(start, off), hm(end, off)),
        None => format!("{}, desde {}", day(start, now, off), hm(start, off)),
    }
}

/// "Desde 21:12" today, "Desde ontem, 21:12" before.
pub fn since(t: DateTime<Utc>, now: DateTime<Utc>, off: FixedOffset) -> String {
    let d = day(t, now, off);
    if d == "hoje" {
        format!("Desde {}", hm(t, off))
    } else {
        format!("Desde {d}, {}", hm(t, off))
    }
}

/// "17 out, 00:16" (the slot list writes the date always).
pub fn played_at(t: DateTime<Utc>, off: FixedOffset) -> String {
    format!("{}, {}", date(t, off), hm(t, off))
}

/// "1h52", "45 min".
pub fn duration(start: DateTime<Utc>, end: DateTime<Utc>) -> String {
    let mins = (end - start).num_minutes().max(0);
    if mins >= 60 {
        format!("{}h{:02}", mins / 60, mins % 60)
    } else {
        format!("{mins} min")
    }
}

fn comma(x: f64, decimals: usize) -> String {
    format!("{x:.decimals$}").replace('.', ",")
}

/// "1,8 MB", "145 KB", "900 B".
pub fn size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= KB * KB {
        format!("{} MB", comma(b / (KB * KB), 1))
    } else if b >= KB {
        format!("{} KB", (b / KB).round() as u64)
    } else {
        format!("{bytes} B")
    }
}

/// "1,1 de 1,8 MB" (the unit of the total).
pub fn progress(done: u64, total: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    if total as f64 >= MB {
        format!("{} de {} MB", comma(done as f64 / MB, 1), comma(total as f64 / MB, 1))
    } else {
        format!("{} de {} KB", (done as f64 / 1024.0).round() as u64, (total as f64 / 1024.0).round() as u64)
    }
}

pub fn percent(done: u64, total: u64) -> u8 {
    if total == 0 {
        0
    } else {
        ((done.min(total) as f64 / total as f64) * 100.0).round() as u8
    }
}

/// The letter of a nick's avatar: its first letter or digit, full-width forms folded ("[ツ] ＬＡＲＡＮＪＯ" -> "L").
pub fn initial(nick: &str) -> String {
    nick.chars()
        .map(|c| if ('\u{ff01}'..='\u{ff5e}').contains(&c) { char::from_u32(c as u32 - 0xfee0).unwrap_or(c) } else { c })
        .find(|c| c.is_alphanumeric() && !matches!(*c, '\u{3040}'..='\u{30ff}'))
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "?".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn br() -> FixedOffset {
        FixedOffset::west_opt(3 * 3600).unwrap()
    }
    fn at(d: u32, h: u32, m: u32) -> DateTime<Utc> {
        br().with_ymd_and_hms(2026, 10, d, h, m, 0).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn days_times_and_ranges() {
        let now = at(21, 22, 0); // a Wednesday
        assert_eq!(range(at(21, 14, 10), Some(at(21, 16, 2)), now, br()), "hoje, 14:10 – 16:02");
        assert_eq!(range(at(20, 21, 48), Some(at(20, 23, 40)), now, br()), "ontem, 21:48 – 23:40");
        assert_eq!(range(at(18, 20, 30), Some(at(18, 23, 58)), now, br()), "dom, 20:30 – 23:58");
        assert_eq!(range(at(21, 21, 12), None, now, br()), "hoje, desde 21:12");
        assert_eq!(day(at(10, 1, 0), now, br()), "10 out");
        assert_eq!(day_time(at(20, 23, 40), now, br()), "ontem, 23:40");
        assert_eq!(since(at(21, 20, 47), now, br()), "Desde 20:47");
        assert_eq!(since(at(20, 20, 47), now, br()), "Desde ontem, 20:47");
        assert_eq!(played_at(at(17, 0, 16), br()), "17 out, 00:16");
        assert_eq!(duration(at(20, 21, 48), at(20, 23, 40)), "1h52");
        assert_eq!(duration(at(20, 21, 0), at(20, 21, 45)), "45 min");
    }

    #[test]
    fn sizes_progress_and_initials() {
        assert_eq!(size(1_797_049), "1,7 MB");
        assert_eq!(size(1_887_437), "1,8 MB");
        assert_eq!(size(148_235), "145 KB");
        assert_eq!(size(12), "12 B");
        assert_eq!(progress(1_153_434, 1_887_437), "1,1 de 1,8 MB");
        assert_eq!(percent(62, 100), 62);
        assert_eq!(percent(1, 0), 0);
        assert_eq!(initial("[ツ] Ｌ　Ａ　Ｒ　Ａ　Ｎ　Ｊ　Ｏ"), "L");
        assert_eq!(initial("rodrigao_br"), "R");
        assert_eq!(initial("[]"), "?");
    }
}
