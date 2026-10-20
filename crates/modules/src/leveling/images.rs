use std::sync::{Arc, OnceLock};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, Tree, fontdb};

#[derive(Debug)]
pub struct RankCardData<'a> {
    pub username: &'a str,
    pub avatar_url: Option<&'a str>,
    pub level: i32,
    pub rank: Option<i64>,
    pub current_xp: i64,
    pub next_level_xp: i64,
    pub message_count: i32,
    pub voice_minutes: i32,
    pub accent: &'a str,
}

#[derive(Debug)]
pub struct LeaderboardRow {
    pub rank: usize,
    pub username: String,
    pub level: i32,
    pub xp: i64,
    pub progress: f64,
}

fn escape_xml(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn format_voice(minutes: i32) -> String {
    if minutes < 60 {
        format!("{minutes}m")
    } else {
        let h = minutes / 60;
        let m = minutes % 60;
        if m == 0 {
            format!("{h}h")
        } else {
            format!("{h}h {m}m")
        }
    }
}

fn get_fontdb() -> Arc<fontdb::Database> {
    static FONTDB: OnceLock<Arc<fontdb::Database>> = OnceLock::new();
    FONTDB
        .get_or_init(|| {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            db.set_sans_serif_family("DejaVu Sans");
            Arc::new(db)
        })
        .clone()
}

pub fn svg_to_png(svg: &str) -> Result<Vec<u8>, anyhow::Error> {
    let opt = Options {
        fontdb: get_fontdb(),
        font_family: "DejaVu Sans".to_string(),
        ..Default::default()
    };
    let tree = Tree::from_str(svg, &opt)?;
    let size = tree.size();
    let width = size.width().ceil() as u32;
    let height = size.height().ceil() as u32;
    let mut pixmap = Pixmap::new(width, height)
        .ok_or_else(|| anyhow::anyhow!("failed to create pixmap with size {width}x{height}"))?;
    resvg::render(&tree, Transform::default(), &mut pixmap.as_mut());
    pixmap.encode_png().map_err(|e| anyhow::anyhow!("{e}"))
}

fn http_client() -> Option<&'static reqwest::Client> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(4))
                .build()
                .ok()
        })
        .as_ref()
}

/// Rewrite a Discord CDN image URL to its PNG raster.
///
/// Serenity hands out WEBP (or GIF) avatar URLs while the data URI built below
/// declares `image/png`; that mismatch made resvg drop the image, so rank cards
/// rendered no avatar at all. Discord serves a static PNG for any avatar hash.
fn png_url(url: &str) -> String {
    let (path, query) = url
        .split_once('?')
        .map_or((url, ""), |(path, query)| (path, query));
    let stem = path
        .strip_suffix(".webp")
        .or_else(|| path.strip_suffix(".gif"))
        .or_else(|| path.strip_suffix(".jpg"))
        .or_else(|| path.strip_suffix(".jpeg"))
        .or_else(|| path.strip_suffix(".png"))
        .unwrap_or(path);
    if query.is_empty() {
        format!("{stem}.png")
    } else {
        format!("{stem}.png?{query}")
    }
}

pub async fn fetch_avatar_base64(url: Option<&str>) -> Option<String> {
    let url = png_url(url?);
    let bytes = http_client()?
        .get(url)
        .send()
        .await
        .ok()?
        .bytes()
        .await
        .ok()?;
    Some(format!("data:image/png;base64,{}", BASE64.encode(&bytes)))
}

pub async fn render_rank_card(data: RankCardData<'_>) -> Result<Vec<u8>, anyhow::Error> {
    let width = 900;
    let height = 300;
    let progress = if data.next_level_xp > 0 {
        (data.current_xp as f64 / data.next_level_xp as f64).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let bar_width = 560;
    let fill_width = (bar_width as f64 * progress)
        .round()
        .clamp(0.0, f64::from(bar_width)) as u32;

    let clean_name = if data.username.chars().count() > 22 {
        let truncated: String = data.username.chars().take(21).collect();
        format!("{truncated}…")
    } else {
        data.username.to_string()
    };
    let name_xml = escape_xml(&clean_name);
    let initial = clean_name.chars().next().unwrap_or('?').to_uppercase();

    let avatar_data = fetch_avatar_base64(data.avatar_url).await;
    let avatar_svg = if let Some(base64) = avatar_data {
        format!(
            r##"<clipPath id="avatar-clip"><circle cx="130" cy="150" r="75"/></clipPath>
               <image href="{base64}" x="55" y="75" width="150" height="150" clip-path="url(#avatar-clip)"/>"##
        )
    } else {
        format!(
            r##"<circle cx="130" cy="150" r="75" fill="{}" opacity="0.35"/>
               <text x="130" y="175" font-family="DejaVu Sans, sans-serif" font-size="70" font-weight="bold" fill="#ffffff" text-anchor="middle">{}</text>"##,
            data.accent, initial
        )
    };

    let rank_str = data
        .rank
        .map_or_else(|| "UNRANKED".to_string(), |r| format!("#{r}"));
    let voice_str = format_voice(data.voice_minutes);

    let svg = format!(
        r##"<svg width="{width}" height="{height}" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#1a1d29"/>
      <stop offset="1" stop-color="#252a3d"/>
    </linearGradient>
    <linearGradient id="bar" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0" stop-color="{accent}"/>
      <stop offset="1" stop-color="#8b9dff"/>
    </linearGradient>
  </defs>
  <rect width="{width}" height="{height}" rx="24" fill="url(#bg)"/>
  <rect width="10" height="{height}" fill="{accent}"/>
  {avatar_svg}
  <text x="240" y="92" font-family="DejaVu Sans, sans-serif" font-size="44" font-weight="bold" fill="#ffffff">{name_xml}</text>
  <text x="240" y="130" font-family="DejaVu Sans, sans-serif" font-size="24" fill="#9aa3b8">LEVEL {}  ·  RANK {rank_str}</text>
  <rect x="240" y="152" width="{bar_width}" height="30" rx="15" fill="#10131c"/>
  <rect x="240" y="152" width="{fill_width}" height="30" rx="15" fill="url(#bar)"/>
  <text x="240" y="212" font-family="DejaVu Sans, sans-serif" font-size="22" fill="#c6cde0">{} / {} XP</text>
  <text x="240" y="252" font-family="DejaVu Sans, sans-serif" font-size="22" fill="#9aa3b8">{} messages  ·  {voice_str} in voice</text>
</svg>"##,
        data.level,
        data.current_xp,
        data.next_level_xp,
        data.message_count,
        accent = data.accent,
    );

    svg_to_png(&svg)
}

pub fn render_leaderboard_card(
    title: &str,
    subtitle: &str,
    rows: &[LeaderboardRow],
    accent: &str,
) -> Result<Vec<u8>, anyhow::Error> {
    let width = 800;
    let row_h = 76;
    let header_h = 108;
    let footer_h = 44;
    let height = header_h + rows.len() * row_h + footer_h;
    let bar_max = width - 180;

    let mut rows_svg = String::new();
    for (i, row) in rows.iter().enumerate() {
        let y = header_h + i * row_h;
        let fill = (bar_max as f64 * row.progress.clamp(0.0, 1.0))
            .round()
            .clamp(0.0, f64::from(bar_max)) as u32;
        let clean_name = if row.username.chars().count() > 22 {
            let truncated: String = row.username.chars().take(21).collect();
            format!("{truncated}…")
        } else {
            row.username.clone()
        };
        let name_xml = escape_xml(&clean_name);

        rows_svg.push_str(&format!(
            r##"<text x="36" y="{}" font-family="DejaVu Sans, sans-serif" font-size="26" font-weight="bold" fill="#7c8499">#{}</text>
  <text x="110" y="{}" font-family="DejaVu Sans, sans-serif" font-size="26" font-weight="bold" fill="#ffffff">{}</text>
  <text x="110" y="{}" font-family="DejaVu Sans, sans-serif" font-size="18" fill="#9aa3b8">Level {}</text>
  <text x="{}" y="{}" font-family="DejaVu Sans, sans-serif" font-size="20" font-weight="bold" fill="#ffffff" text-anchor="end">{} XP</text>
  <rect x="110" y="{}" width="{bar_max}" height="8" rx="4" fill="#10131c"/>
  <rect x="110" y="{}" width="{fill}" height="8" rx="4" fill="{accent}"/>
"##,
            y + 42,
            row.rank,
            y + 30,
            name_xml,
            y + 54,
            row.level,
            width - 36,
            y + 36,
            row.xp,
            y + 64,
            y + 64,
        ));
    }

    let title_xml = escape_xml(title);
    let subtitle_xml = escape_xml(subtitle);

    let svg = format!(
        r##"<svg width="{width}" height="{height}" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#1a1d29"/>
      <stop offset="1" stop-color="#252a3d"/>
    </linearGradient>
  </defs>
  <rect width="{width}" height="{height}" rx="24" fill="url(#bg)"/>
  <rect width="10" height="{height}" fill="{accent}"/>
  <text x="36" y="52" font-family="DejaVu Sans, sans-serif" font-size="34" font-weight="bold" fill="#ffffff">{title_xml}</text>
  <text x="36" y="84" font-family="DejaVu Sans, sans-serif" font-size="20" fill="#9aa3b8">{subtitle_xml}</text>
  {rows_svg}
  <text x="36" y="{}" font-family="DejaVu Sans, sans-serif" font-size="16" fill="#7c8499">Top {} server members</text>
</svg>"##,
        height - 16,
        rows.len(),
    );

    svg_to_png(&svg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_rank_svg_to_png() {
        let svg = r##"<svg width="200" height="100" xmlns="http://www.w3.org/2000/svg">
            <rect width="200" height="100" fill="#1a1d29"/>
            <text x="20" y="50" font-family="DejaVu Sans, sans-serif" font-size="20" fill="#ffffff">Test</text>
        </svg>"##;
        let Ok(png) = svg_to_png(svg) else {
            panic!("render should succeed");
        };
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
    }

    #[test]
    fn avatar_url_is_forced_to_png() {
        assert_eq!(
            png_url("https://cdn.discordapp.com/avatars/1/abc.webp?size=1024"),
            "https://cdn.discordapp.com/avatars/1/abc.png?size=1024"
        );
        assert_eq!(
            png_url("https://cdn.discordapp.com/avatars/1/a_abc.gif"),
            "https://cdn.discordapp.com/avatars/1/a_abc.png"
        );
        assert_eq!(
            png_url("https://cdn.discordapp.com/embed/avatars/0.png"),
            "https://cdn.discordapp.com/embed/avatars/0.png"
        );
    }

    #[test]
    fn avatar_image_is_clipped_to_a_circle() {
        use resvg::tiny_skia::{Color, Pixmap};
        let Some(mut source) = Pixmap::new(150, 150) else {
            panic!("pixmap");
        };
        source.fill(Color::from_rgba8(255, 0, 0, 255));
        let Some(png) = source.encode_png().ok() else {
            panic!("encode");
        };
        let avatar = format!(
            r##"<clipPath id="avatar-clip"><circle cx="130" cy="150" r="75"/></clipPath>
               <image href="data:image/png;base64,{}" x="55" y="75" width="150" height="150" clip-path="url(#avatar-clip)"/>"##,
            BASE64.encode(png)
        );
        let svg = format!(
            r##"<svg width="300" height="300" xmlns="http://www.w3.org/2000/svg">{avatar}</svg>"##
        );
        let Ok(rendered) = svg_to_png(&svg)
            .and_then(|png| Pixmap::decode_png(&png).map_err(|err| anyhow::anyhow!("{err}")))
        else {
            panic!("render");
        };
        let (Some(corner), Some(center)) = (rendered.pixel(60, 80), rendered.pixel(130, 150))
        else {
            panic!("pixels");
        };
        assert_eq!(corner.alpha(), 0, "corner clipped away");
        assert_eq!(center.alpha(), 255, "center painted");
    }

    #[test]
    fn test_render_leaderboard_card() {
        let rows = vec![
            LeaderboardRow {
                rank: 1,
                username: "Alice".to_string(),
                level: 10,
                xp: 5000,
                progress: 0.8,
            },
            LeaderboardRow {
                rank: 2,
                username: "Bob".to_string(),
                level: 8,
                xp: 3200,
                progress: 0.5,
            },
        ];
        let Ok(png) = render_leaderboard_card("Leaderboard", "Top members", &rows, "#6366f1")
        else {
            panic!("leaderboard render should succeed");
        };
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
    }
}
