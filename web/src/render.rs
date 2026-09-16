use maud::{DOCTYPE, Markup, PreEscaped, html};
use pulldown_cmark::html as markdown_html;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag};

use crate::assets::{
    Fingerprint, LOGO_PATH, MASCOT_PATH, OG_IMAGE_PATH, SCREENSHOT_PATH, STYLESHEET_PATH,
};
use crate::github::{REPOSITORY_URL, Release, latest_with_download};

const DESCRIPTION: &str = "Cadence is a minimal Spotify player for macOS.";
const OG_IMAGE_ALT: &str = "Cadence showing the Liked Songs library";

fn releases_url() -> String {
    format!("{REPOSITORY_URL}/releases")
}

pub fn home(releases: &[Release], fingerprint: &Fingerprint) -> String {
    let latest = latest_with_download(releases);
    layout(
        "/",
        "Cadence: a minimal Spotify player for macOS",
        fingerprint,
        html! {
            section .hero {
                img .hero-logo src=(fingerprint.url(LOGO_PATH)) alt="" width="88" height="88";
                h1 { "Cadence" }
                p .tagline { "A minimal Spotify player for macOS." }
                p .subtitle { "Native, responsive, and typically uses around 120 MB of RAM." }
                p .actions {
                    a .button href="/download" {
                        (download_icon())
                        span { "Download for macOS" }
                    }
                }
                p .meta {
                    @if let Some(release) = latest {
                        span { "v" (release.version()) " · " }
                    }
                    span { "Signed and notarized" }
                }
            }
            section .shot {
                div .shot-frame {
                    img .screenshot src=(fingerprint.url(SCREENSHOT_PATH))
                        alt="Cadence showing the Liked Songs library"
                        width="2784" height="1824";
                    img .mascot src=(fingerprint.url(MASCOT_PATH)) alt="" width="16" height="16";
                }
            }
        },
    )
}

pub fn releases(releases: &[Release], fingerprint: &Fingerprint) -> String {
    layout(
        "/releases",
        "Release Notes · Cadence",
        fingerprint,
        html! {
            header .page-head {
                h1 { "Release Notes" }
                p { "Every release of Cadence, newest first." }
            }
            @if releases.is_empty() {
                p .empty {
                    "Release notes are unavailable right now. See them on "
                    a href=(releases_url()) { "GitHub" } "."
                }
            } @else {
                div .releases {
                    @for (index, release) in releases.iter().enumerate() {
                        (release_item(release, index == 0))
                    }
                }
            }
        },
    )
}

fn release_item(release: &Release, latest: bool) -> Markup {
    html! {
        article .release {
            div .release-head {
                h2 {
                    (release.tag_name)
                    @if latest {
                        span .badge { "Latest" }
                    }
                }
                @if let Some(date) = release.published_at.as_deref() {
                    time datetime=(date) { (format_date(date)) }
                }
            }
            div .release-notes {
                (markdown(release.body.as_deref().unwrap_or_default()))
            }
            @if let Some(url) = release.dmg_url() {
                p .release-download {
                    a href=(url) { "Download " (release.tag_name) }
                }
            }
        }
    }
}

fn layout(path: &str, title: &str, fingerprint: &Fingerprint, content: Markup) -> String {
    let site = site_url();
    let page_url = format!("{site}{path}");
    let image_url = format!("{site}{}", fingerprint.url(OG_IMAGE_PATH));

    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                meta name="description" content=(DESCRIPTION);
                title { (title) }
                link rel="canonical" href=(page_url);
                link rel="icon" type="image/svg+xml" href=(fingerprint.url(LOGO_PATH));
                link rel="stylesheet" href=(fingerprint.url(STYLESHEET_PATH));
                meta name="theme-color" content="#fbfaf9" media="(prefers-color-scheme: light)";
                meta name="theme-color" content="#121212" media="(prefers-color-scheme: dark)";
                meta property="og:type" content="website";
                meta property="og:site_name" content="Cadence";
                meta property="og:title" content=(title);
                meta property="og:description" content=(DESCRIPTION);
                meta property="og:url" content=(page_url);
                meta property="og:image" content=(image_url);
                meta property="og:image:width" content="1200";
                meta property="og:image:height" content="630";
                meta property="og:image:alt" content=(OG_IMAGE_ALT);
                meta name="twitter:card" content="summary_large_image";
                meta name="twitter:title" content=(title);
                meta name="twitter:description" content=(DESCRIPTION);
                meta name="twitter:image" content=(image_url);
                meta name="twitter:image:alt" content=(OG_IMAGE_ALT);
            }
            body {
                (nav(fingerprint))
                main { (content) }
                (footer())
            }
        }
    }
    .into_string()
}

fn nav(fingerprint: &Fingerprint) -> Markup {
    html! {
        header .nav {
            div .nav-inner {
                a .nav-brand href="/" {
                    img .nav-logo src=(fingerprint.url(LOGO_PATH)) alt="" width="24" height="24";
                    span { "Cadence" }
                }
                nav .nav-links {
                    a href="/releases" { "Release Notes" }
                    a href=(REPOSITORY_URL) { "GitHub" }
                }
            }
        }
    }
}

fn footer() -> Markup {
    html! {
        footer .footer {
            p { "Cadence is free and open source under the MIT License." }
            p {
                a href="/releases" { "Release Notes" }
                " · "
                a href=(REPOSITORY_URL) { "GitHub" }
            }
        }
    }
}

fn download_icon() -> Markup {
    html! {
        svg .icon viewBox="0 0 24 24" width="18" height="18"
            fill="none" stroke="currentColor" stroke-width="1.8"
            stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" {
            path d="M12 4v11" {}
            path d="m7.5 10.5 4.5 4.5 4.5-4.5" {}
            path d="M5 19h14" {}
        }
    }
}

fn markdown(source: &str) -> Markup {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_GFM);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_STRIKETHROUGH);

    let source = link_full_changelog(source);
    let parser = Parser::new_ext(&source, options).map(demote_heading);

    let mut output = String::new();
    markdown_html::push_html(&mut output, parser);
    PreEscaped(output)
}

/// GitHub appends a bare compare URL that CommonMark leaves as plain text.
/// Angle brackets make it a native autolink, so code spans and markdown links
/// keep working without a custom event pipeline.
fn link_full_changelog(source: &str) -> String {
    const PREFIX: &str = "**Full Changelog**: ";

    source
        .lines()
        .map(|line| match line.strip_prefix(PREFIX) {
            Some(url) if is_bare_url(url) => format!("{PREFIX}<{url}>"),
            _ => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_bare_url(value: &str) -> bool {
    value.starts_with("https://")
        && !value.contains(|character: char| {
            character.is_whitespace() || matches!(character, '<' | '>' | '(' | ')' | '[' | ']')
        })
}

/// Release titles render as `h2`, so push body headings one level down to keep
/// the document outline nested under them.
fn demote_heading<'a>(event: Event<'a>) -> Event<'a> {
    match event {
        Event::Start(Tag::Heading {
            level,
            id,
            classes,
            attrs,
        }) => Event::Start(Tag::Heading {
            level: next_level(level),
            id,
            classes,
            attrs,
        }),
        other => other,
    }
}

fn next_level(level: HeadingLevel) -> HeadingLevel {
    match level {
        HeadingLevel::H1 => HeadingLevel::H2,
        HeadingLevel::H2 => HeadingLevel::H3,
        HeadingLevel::H3 => HeadingLevel::H4,
        HeadingLevel::H4 => HeadingLevel::H5,
        HeadingLevel::H5 | HeadingLevel::H6 => HeadingLevel::H6,
    }
}

fn site_url() -> String {
    std::env::var("SITE_URL").unwrap_or_else(|_| "https://cadence.miho.dev".to_string())
}

fn format_date(iso: &str) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];

    let date = iso.get(..10).unwrap_or(iso);
    let mut parts = date.split('-');
    let (Some(year), Some(month), Some(day)) = (parts.next(), parts.next(), parts.next()) else {
        return iso.to_string();
    };
    let Ok(month) = month.parse::<usize>() else {
        return iso.to_string();
    };
    let Some(name) = MONTHS.get(month.wrapping_sub(1)) else {
        return iso.to_string();
    };

    format!("{name} {}, {year}", day.trim_start_matches('0'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASE_BODY: &str = "## What's new\n\n- **One update setting.** The window no longer installs silently.\n\n**Full Changelog**: https://github.com/infomiho/cadence/compare/v0.6.0...v0.6.1\n";

    #[test]
    fn renders_a_release_body_end_to_end() {
        let rendered = markdown(RELEASE_BODY).into_string();
        assert!(
            rendered.contains(
                "<a href=\"https://github.com/infomiho/cadence/compare/v0.6.0...v0.6.1\">"
            ),
            "{rendered}"
        );
        assert!(rendered.starts_with("<h3>"), "{rendered}");
    }

    #[test]
    fn keeps_an_existing_markdown_link_untouched() {
        let rendered =
            markdown("**Full Changelog**: [compare](https://example.com/c)").into_string();
        assert!(
            rendered.contains("<a href=\"https://example.com/c\">compare</a>"),
            "{rendered}"
        );
    }

    #[test]
    fn leaves_unrelated_lines_alone() {
        assert_eq!(link_full_changelog("nothing to see"), "nothing to see");
    }

    #[test]
    fn formats_release_dates() {
        assert_eq!(format_date("2026-09-10T18:12:09Z"), "September 10, 2026");
        assert_eq!(format_date("2026-01-05T00:00:00Z"), "January 5, 2026");
        assert_eq!(format_date("unknown"), "unknown");
    }
}
