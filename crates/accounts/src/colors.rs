//! Account colors (PLAN §4.5): tint the repo tab/window border and the account pill so the
//! user sees at a glance whether they are in a "work" or "personal" context.
//!
//! The palette is chosen so that every color is clearly distinct from the others and from the
//! semantic success/danger colors of both themes (`apps/desktop/src/theme/tokens.css`) — an
//! account must never look like an error or "all good" badge. Distance is CIE76 ΔE in CIELAB;
//! a test re-reads `tokens.css` so a palette change there cannot silently break this.

/// Default account colors, assigned in order to new accounts.
pub const ACCOUNT_COLORS: &[&str] = &[
    "#1c7ed6", // blue
    "#7048e8", // violet
    "#0c8599", // teal
    "#e64980", // pink
    "#f08c00", // amber
    "#ae3ec9", // grape
    "#5c7cfa", // periwinkle
    "#8d6e63", // brown
];

/// Minimum ΔE between two palette colors, and between a palette color and success/danger.
pub const MIN_DELTA_E: f64 = 20.0;

pub fn is_hex_color(s: &str) -> bool {
    parse_hex(s).is_some()
}

/// `#rrggbb` → (r, g, b).
pub fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some((byte(0)?, byte(2)?, byte(4)?))
}

/// First palette color not in `used` (case-insensitive); cycles when all are taken.
pub fn next_color<'a>(used: impl IntoIterator<Item = &'a str>) -> &'static str {
    let used: Vec<String> = used.into_iter().map(str::to_ascii_lowercase).collect();
    ACCOUNT_COLORS
        .iter()
        .find(|c| !used.iter().any(|u| u == *c))
        .copied()
        .unwrap_or(ACCOUNT_COLORS[used.len() % ACCOUNT_COLORS.len()])
}

fn to_lab((r, g, b): (u8, u8, u8)) -> [f64; 3] {
    fn lin(c: u8) -> f64 {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    let (r, g, b) = (lin(r), lin(g), lin(b));
    // sRGB → XYZ (D65), normalized by the reference white.
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.950_47;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.088_83;
    let f = |t: f64| {
        if t > 216.0 / 24389.0 {
            t.cbrt()
        } else {
            (24389.0 / 27.0 * t + 16.0) / 116.0
        }
    };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// CIE76 color difference between two `#rrggbb` colors; `None` if either is malformed.
pub fn delta_e(a: &str, b: &str) -> Option<f64> {
    let (a, b) = (to_lab(parse_hex(a)?), to_lab(parse_hex(b)?));
    Some(((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `--<name>: #xxxxxx` value in tokens.css (both themes).
    fn token_values(name: &str) -> Vec<String> {
        let css = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../apps/desktop/src/theme/tokens.css"),
        )
        .expect("read tokens.css");
        let prefix = format!("--{name}:");
        css.lines()
            .filter_map(|l| l.trim().strip_prefix(&prefix))
            .map(|v| v.trim().trim_end_matches(';').trim().to_ascii_lowercase())
            .collect()
    }

    #[test]
    fn palette_is_valid_and_distinct() {
        for (i, a) in ACCOUNT_COLORS.iter().enumerate() {
            assert!(is_hex_color(a), "{a}");
            assert_eq!(*a, a.to_ascii_lowercase());
            for b in &ACCOUNT_COLORS[i + 1..] {
                let d = delta_e(a, b).unwrap();
                assert!(d >= MIN_DELTA_E, "{a} vs {b}: ΔE {d:.1}");
            }
        }
    }

    #[test]
    fn palette_never_looks_like_success_or_danger() {
        let semantic: Vec<String> = ["success", "danger"]
            .iter()
            .flat_map(|n| token_values(n))
            .collect();
        assert_eq!(
            semantic.len(),
            4,
            "expected light+dark success/danger: {semantic:?}"
        );
        for a in ACCOUNT_COLORS {
            for s in &semantic {
                assert_ne!(a, s);
                let d = delta_e(a, s).unwrap();
                assert!(d >= MIN_DELTA_E, "{a} too close to semantic {s}: ΔE {d:.1}");
            }
        }
    }

    #[test]
    fn helpers() {
        assert_eq!(parse_hex("#FF8000"), Some((255, 128, 0)));
        assert_eq!(parse_hex("ff8000"), None);
        assert_eq!(parse_hex("#ff80"), None);
        assert_eq!(parse_hex("#gg0000"), None);
        assert!(delta_e("#000000", "#ffffff").unwrap() > 99.0);
        assert_eq!(delta_e("#123456", "#123456"), Some(0.0));
        assert_eq!(next_color([]), ACCOUNT_COLORS[0]);
        assert_eq!(next_color(["#1C7ED6"]), ACCOUNT_COLORS[1]);
        assert_eq!(
            next_color(ACCOUNT_COLORS.iter().copied()),
            ACCOUNT_COLORS[0]
        );
    }
}
