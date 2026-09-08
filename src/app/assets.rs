use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

/// Cadence's own SVG icons, with gpui-kit's bundle behind them for the icons
/// the kit's components request.
#[derive(rust_embed::RustEmbed)]
#[folder = "assets/icons"]
#[prefix = "icons/"]
pub(super) struct CadenceAssets;

impl AssetSource for CadenceAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match Self::get(path) {
            Some(file) => Ok(Some(file.data)),
            None => gpui_kit::assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut entries: Vec<SharedString> = Self::iter()
            .filter(|entry| entry.starts_with(path))
            .map(SharedString::from)
            .collect();
        entries.extend(gpui_kit::assets::Assets.list(path)?);
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::icons::CadenceIcon;
    use gpui_kit::component::IconNamed;

    #[test]
    fn every_icon_is_bundled() {
        let bundled = CadenceAssets::iter()
            .filter(|path| path.starts_with("icons/cadence/"))
            .count();
        assert_eq!(
            bundled,
            CadenceIcon::ALL.len(),
            "icon list and bundle differ"
        );
        for icon in CadenceIcon::ALL {
            let path = icon.path();
            assert!(
                CadenceAssets.load(&path).is_ok_and(|data| data.is_some()),
                "missing {path}"
            );
        }
    }

    #[test]
    fn kit_icons_still_resolve_through_the_fallback() {
        assert!(
            CadenceAssets
                .load("icons/check.svg")
                .is_ok_and(|data| data.is_some())
        );
    }
}
