use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub const STYLESHEET: &str = include_str!("../static/style.css");
pub const LOGO: &[u8] = include_bytes!("../../assets/cadence-mark.svg");
pub const SCREENSHOT: &[u8] = include_bytes!("../../assets/cadence.webp");
pub const OG_IMAGE: &[u8] = include_bytes!("../static/og.png");
pub const MASCOT: &[u8] = include_bytes!("../static/mascot.svg");

pub const LOGO_PATH: &str = "/cadence-mark.svg";
pub const SCREENSHOT_PATH: &str = "/cadence.webp";
pub const STYLESHEET_PATH: &str = "/style.css";
pub const OG_IMAGE_PATH: &str = "/og.png";
pub const MASCOT_PATH: &str = "/mascot.svg";

/// Cache-busting token derived from the embedded assets. Asset URLs carry it as
/// `?v=`, so the files can be served as immutable while a new deployment busts
/// the cache automatically.
pub struct Fingerprint(u64);

impl Fingerprint {
    pub fn new() -> Self {
        let mut hasher = DefaultHasher::new();
        for asset in [STYLESHEET.as_bytes(), LOGO, SCREENSHOT, OG_IMAGE, MASCOT] {
            asset.hash(&mut hasher);
        }
        Self(hasher.finish())
    }

    pub fn url(&self, path: &str) -> String {
        format!("{path}?v={:016x}", self.0)
    }
}
