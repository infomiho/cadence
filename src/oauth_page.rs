use base64::Engine as _;

pub(crate) enum OAuthStep {
    Library,
    Playback,
}

pub(crate) fn success_page(step: OAuthStep, next_url: Option<&str>) -> String {
    let (step_label, title, description, action) = match (step, next_url) {
        (OAuthStep::Library, Some(next_url)) => (
            "Step 1 of 2",
            "Library connected",
            "Continue to approve playback. Spotify will ask you to authorize Cadence once more.",
            format!("<a href=\"{}\">Next step</a>", escape_attribute(next_url)),
        ),
        (OAuthStep::Library, None) => (
            "Authorization complete",
            "Cadence is ready",
            "Authorization is complete. Close this page and return to Cadence.",
            String::new(),
        ),
        (OAuthStep::Playback, _) => (
            "Step 2 of 2",
            "Cadence is ready",
            "Playback is connected. Close this page and return to Cadence.",
            String::new(),
        ),
    };
    let brand_mark = base64::engine::general_purpose::STANDARD
        .encode(include_bytes!("../assets/cadence-mark.png"));

    format!(
        r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>{title} - Cadence</title>
  <style>
    :root {{ color-scheme: light dark; font-family: Inter, ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }}
    * {{ box-sizing: border-box; }}
    body {{ min-height: 100vh; margin: 0; display: grid; place-items: center; background: #fbfaf9; color: #171717; }}
    main {{ width: min(480px, calc(100vw - 40px)); padding: 32px; border: 1px solid #e8e8e8; border-radius: 16px; background: #fff; box-shadow: 0 12px 32px rgba(0,0,0,.06); }}
    .brand {{ margin-bottom: 40px; display: flex; align-items: center; gap: 10px; font-size: 17px; font-weight: 700; }}
    .brand img {{ width: 32px; height: 32px; border-radius: 10px; }}
    .step {{ margin: 0 0 10px; color: #494440; font-size: 13px; font-weight: 650; }}
    h1 {{ margin: 0; font-size: 28px; line-height: 1.15; letter-spacing: -.02em; }}
    p {{ margin: 12px 0 0; color: #494440; font-size: 15px; line-height: 1.55; }}
    a {{ width: fit-content; margin-top: 28px; min-height: 42px; padding: 0 16px; border: 1px solid #171717; border-radius: 10px; background: #171717; color: #fff; font: inherit; font-size: 14px; font-weight: 650; cursor: pointer; display: flex; align-items: center; text-decoration: none; }}
    a:hover {{ background: #303030; }}
    a:focus-visible {{ outline: 2px solid #848281; outline-offset: 3px; }}
    @media (prefers-color-scheme: dark) {{
      body {{ background: #121212; color: #f5f3ef; }}
      main {{ border-color: #414141; background: #1a1a1a; box-shadow: 0 20px 60px rgba(0,0,0,.35); }}
      .step, p {{ color: #d5d1cb; }}
      a {{ border-color: #f5f3ef; background: #f5f3ef; color: #171717; }}
      a:hover {{ background: #d5d1cb; }}
      a:focus-visible {{ outline-color: #a8a5a1; }}
    }}
  </style>
</head>
<body>
  <main>
    <div class="brand"><img src="data:image/png;base64,{brand_mark}" alt="">Cadence</div>
    <div class="step">{step_label}</div>
    <h1>{title}</h1>
    <p>{description}</p>
    {action}
  </main>
</body>
</html>"##
    )
}

fn escape_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::{OAuthStep, success_page};

    #[test]
    fn callback_pages_explain_both_authorization_steps() {
        let library = success_page(
            OAuthStep::Library,
            Some("https://accounts.spotify.com/authorize?a=1&b=2"),
        );
        assert!(library.contains("Step 1 of 2"));
        assert!(library.contains("Next step"));
        assert!(library.contains("a=1&amp;b=2"));
        assert!(!library.contains("setTimeout"));

        let playback = success_page(OAuthStep::Playback, None);
        assert!(playback.contains("Step 2 of 2"));
        assert!(playback.contains("Close this page"));
        assert!(!playback.contains("<button"));
        assert!(!playback.contains("setTimeout"));
    }
}
