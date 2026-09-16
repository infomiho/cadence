use std::time::Duration;

use serde::Deserialize;

pub const DEFAULT_REPO: &str = "infomiho/cadence";
pub const REPOSITORY_URL: &str = "https://github.com/infomiho/cadence";
pub const LATEST_RELEASE_URL: &str = "https://github.com/infomiho/cadence/releases/latest";

#[derive(Clone)]
pub struct Github {
    client: reqwest::Client,
    repo: String,
    token: Option<String>,
}

impl Github {
    pub fn from_env() -> Self {
        let repo = std::env::var("GITHUB_REPO").unwrap_or_else(|_| DEFAULT_REPO.to_string());
        let token = std::env::var("GITHUB_TOKEN")
            .or_else(|_| std::env::var("GH_TOKEN"))
            .ok();
        let client = reqwest::Client::builder()
            .user_agent(concat!("cadence-web/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(15))
            .build()
            .expect("build GitHub client");

        Self {
            client,
            repo,
            token,
        }
    }

    pub async fn releases(&self) -> Result<Vec<Release>, reqwest::Error> {
        let url = format!(
            "https://api.github.com/repos/{}/releases?per_page=100",
            self.repo
        );
        let mut request = self
            .client
            .get(url)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }

        let releases: Vec<Release> = request.send().await?.error_for_status()?.json().await?;
        Ok(releases
            .into_iter()
            .filter(|release| !release.draft && !release.prerelease)
            .collect())
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub body: Option<String>,
    pub published_at: Option<String>,
    pub draft: bool,
    pub prerelease: bool,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
}

impl Release {
    pub fn dmg_url(&self) -> Option<&str> {
        self.assets
            .iter()
            .find(|asset| asset.name.ends_with(".dmg"))
            .map(|asset| asset.browser_download_url.as_str())
    }

    pub fn version(&self) -> &str {
        self.tag_name.strip_prefix('v').unwrap_or(&self.tag_name)
    }
}

pub fn latest_with_download(releases: &[Release]) -> Option<&Release> {
    releases.iter().find(|release| release.dmg_url().is_some())
}
