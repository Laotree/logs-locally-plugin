use chrono::{Datelike, Duration, Local, NaiveDate};
use std::collections::HashMap;

/// Payload stored after a push (aggregated only — no raw session content).
#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
pub struct ActivityData {
    pub days: Vec<DayRecord>,
    pub updated_at: String,
}

/// One day's aggregated counts — the only data ever pushed to the public server.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct DayRecord {
    pub day: String,           // "2025-05-27"
    pub session_count: i64,
    pub token_count: i64,
}

/// Render two contribution-style heatmaps (sessions + tokens) as a dark SVG.
pub fn render_svg(data: &ActivityData) -> String {
    let session_map: HashMap<&str, i64> =
        data.days.iter().map(|d| (d.day.as_str(), d.session_count)).collect();
    let token_map: HashMap<&str, i64> =
        data.days.iter().map(|d| (d.day.as_str(), d.token_count)).collect();

    let max_sessions = session_map.values().copied().max().unwrap_or(1).max(1);
    let max_tokens   = token_map.values().copied().max().unwrap_or(1).max(1);

    // Date range: last 365 days aligned to the preceding Sunday
    let today = Local::now().date_naive();
    let start = today - Duration::days(364);
    let dow   = start.weekday().num_days_from_sunday();
    let aligned_start = start - Duration::days(dow as i64);

    // Build week columns: each entry is 7 Option<NaiveDate>
    let mut weeks: Vec<[Option<NaiveDate>; 7]> = Vec::new();
    let mut cur = aligned_start;
    while cur <= today {
        let mut week = [None; 7];
        for slot in &mut week {
            if cur >= start && cur <= today {
                *slot = Some(cur);
            }
            cur += Duration::days(1);
        }
        weeks.push(week);
    }

    // SVG layout constants
    let cell: f32 = 11.0;
    let gap:  f32 =  2.0;
    let stride = cell + gap;

    let left_margin:  f32 = 10.0;
    let label_w:      f32 = 20.0;   // width reserved for M/W/F labels
    let right_margin: f32 = 12.0;
    let grid_x = left_margin + label_w;

    let n_weeks   = weeks.len() as f32;
    let grid_w    = n_weeks * stride - gap;
    let grid_h    = 7.0 * stride - gap;

    let svg_w = (grid_x + grid_w + right_margin).ceil() as u32;

    // Vertical layout
    let top_margin:    f32 = 14.0;
    let section_lbl_h: f32 = 14.0;
    let month_row_h:   f32 = 13.0;
    let between:       f32 = 18.0;
    let footer_h:      f32 = 18.0;
    let bottom_margin: f32 = 10.0;

    let hm1_lbl_y  = top_margin;
    let hm1_grid_y = hm1_lbl_y + section_lbl_h + month_row_h;

    let hm2_lbl_y  = hm1_grid_y + grid_h + between;
    let hm2_grid_y = hm2_lbl_y + section_lbl_h + month_row_h;

    let footer_y = hm2_grid_y + grid_h + between * 0.6;
    let svg_h = (footer_y + footer_h + bottom_margin).ceil() as u32;

    // Helpers
    let months = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"];
    let day_labels = ["S","M","T","W","T","F","S"];

    let level = |val: i64, max: i64| -> usize {
        if val == 0 { return 0; }
        let r = val as f64 / max as f64;
        if r < 0.15 { 1 } else if r < 0.40 { 2 } else if r < 0.70 { 3 } else { 4 }
    };

    let cell_fill = |lv: usize, hex: &str| -> String {
        let (r, g, b) = hex_rgb(hex);
        match lv {
            0 => "rgba(255,255,255,0.05)".into(),
            1 => format!("rgba({r},{g},{b},0.22)"),
            2 => format!("rgba({r},{g},{b},0.48)"),
            3 => format!("rgba({r},{g},{b},0.73)"),
            _ => hex.to_string(),
        }
    };

    // Render one heatmap section
    let render_section = |label: &str, color: &str, map: &HashMap<&str, i64>,
                           max_v: i64, lbl_y: f32, grid_y: f32| -> String {
        let mut s = String::new();

        // Section label
        s.push_str(&format!(
            r#"<text x="{grid_x}" y="{y:.1}" fill="{color}" \
font-family="monospace" font-size="10" font-weight="600" \
letter-spacing="2" opacity="0.75">{label}</text>"#,
            y = lbl_y + 10.0,
        ));

        // Month labels (only first occurrence per month)
        let mut last_month = 99u32;
        for (wi, week) in weeks.iter().enumerate() {
            if let Some(d) = week.iter().flatten().next() {
                let m = d.month0();
                if m != last_month {
                    let x = grid_x + wi as f32 * stride;
                    s.push_str(&format!(
                        r#"<text x="{x:.1}" y="{y:.1}" \
fill="rgba(255,255,255,0.35)" font-family="monospace" font-size="9">{}</text>"#,
                        months[m as usize],
                        y = lbl_y + section_lbl_h + 10.0,
                    ));
                    last_month = m;
                }
            }
        }

        // Day labels (M, W, F = rows 1, 3, 5)
        for di in [1usize, 3, 5] {
            let y = grid_y + di as f32 * stride + cell * 0.78;
            s.push_str(&format!(
                r#"<text x="{x:.1}" y="{y:.1}" \
fill="rgba(255,255,255,0.28)" font-family="monospace" font-size="9" \
text-anchor="end">{}</text>"#,
                day_labels[di],
                x = grid_x - 4.0,
            ));
        }

        // Grid cells
        for (wi, week) in weeks.iter().enumerate() {
            let wx = grid_x + wi as f32 * stride;
            for (di, day_opt) in week.iter().enumerate() {
                if let Some(d) = day_opt {
                    let iso = d.format("%Y-%m-%d").to_string();
                    let val = map.get(iso.as_str()).copied().unwrap_or(0);
                    let lv  = level(val, max_v);
                    let fill = cell_fill(lv, color);
                    let wy = grid_y + di as f32 * stride;
                    s.push_str(&format!(
                        r#"<rect x="{wx:.1}" y="{wy:.1}" width="{cell}" height="{cell}" rx="2" fill="{fill}"/>"#,
                        cell = cell as u32,
                    ));
                }
            }
        }

        s
    };

    let s1 = render_section("SESSIONS", "#f5b942", &session_map, max_sessions,
                             hm1_lbl_y, hm1_grid_y);
    let s2 = render_section("TOKENS",   "#7aa2ff", &token_map,   max_tokens,
                             hm2_lbl_y, hm2_grid_y);

    let updated = if data.updated_at.is_empty() { "—".to_string() } else { data.updated_at[..10.min(data.updated_at.len())].to_string() };

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{svg_w}" height="{svg_h}" style="background:#0d1017;border-radius:6px">
{s1}
{s2}
<text x="{gx:.1}" y="{fy:.1}" fill="rgba(255,255,255,0.18)" font-family="monospace" font-size="8">updated {updated}</text>
</svg>"#,
        gx = grid_x,
        fy = footer_y + 10.0,
    )
}

fn hex_rgb(hex: &str) -> (u8, u8, u8) {
    let h = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0);
    (r, g, b)
}
