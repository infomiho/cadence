# GPUI icon approaches

Researched 2026-09-07. Question: how do GPUI applications implement icons, and what would a move from runtime SF Symbols to bundled SVG icons look like for Cadence? Sources are the Zed, gpui-kit and third-party app repositories, the gpui-pre 0.3.4 and gpui-kit 0.6.0 crate sources in the local cargo registry, Apple's developer documentation and WWDC transcripts, and the license text shipped inside the SF Symbols 7 installer.

## Summary

- Every GPUI app surveyed, including Zed, ships monochrome SVG files inside the binary and renders them with `svg().path()`. gpui rasterises the SVG with resvg, keeps only the alpha channel, and tints it with the element's text colour. Fill and stroke colours inside the file are ignored. [gpui-pre svg_renderer.rs](#5-how-gpuis-svg-element-renders)
- Zed keeps a hand-written `IconName` enum in a dedicated `icons` crate, derives file paths from variant names with strum, embeds `assets/icons` with rust-embed, and documents its icon rules in `crates/icons/README.md`: 16x16 viewBox, 1.2px stroke, 12x12 internal safe area, Lucide as the main source. [Zed section](#1-zed)
- gpui-kit 0.6.0 generates `IconName` from the 101 Lucide SVGs in gpui-kit-assets with the `icon_named!` proc macro. The macro records paths only, never bytes; the bytes come from the single `AssetSource` registered with the application. There is no `Icon::data` in 0.6.0; it exists only on the git main branch. [gpui-kit section](#2-gpui-kit-and-gpui-component)
- Third-party apps converge on the same pattern: rust-embed plus `AssetSource`, Lucide or Tabler or Hugeicons, and either their own enum or gpui-kit's `Icon::empty().path()`. Apps that extend gpui-kit write an `AssetSource` that tries their own files first and falls back to `gpui_kit::assets::Assets`. [Apps section](#3-other-open-source-gpui-apps)
- Only two codebases render SF Symbols at runtime for GPUI: gpui-symbols, which Cadence uses, and native-theme-gpui. gpui-symbols has had no release since 2026-01-21, targets crates.io gpui 0.2.2 rather than gpui-pre, and hardcodes a 2x rasterisation scale. [SF Symbols runtime section](#4-runtime-sf-symbols-in-gpui)
- Exporting SF Symbols to SVG is an official SF Symbols app feature, but the license only permits distributing exported templates "exclusively through the use of Apple's Xcode application", and otherwise forbids symbols being "extracted, copied, modified, distributed, embedded or repackaged". A GPUI app cannot meet that condition. Runtime rendering through AppKit stays within the Xcode SDK agreement's "System-Provided Images" grant for Apple platform apps. [License section](#7-sf-symbols-license)
- Recommendation for Cadence: keep runtime SF Symbols only if a maintained gpui-pre compatible renderer is acceptable; otherwise adopt Lucide from gpui-kit-assets, extend it with a small Cadence-drawn set following Zed's normalisation rules, and drop the gpui-symbols dependency. Exporting SF Symbols to SVG is not a viable option under the license. [Implications](#implications-for-cadence)

## 1. Zed

### Icon names

Zed's icon enum lives in a dedicated crate, `crates/icons/src/icons.rs`. It is hand-written PascalCase, and file paths are derived from the variant name with strum's `IntoStaticStr` and `serialize_all = "snake_case"`:

```rust
#[derive(Debug, PartialEq, Eq, Copy, Clone, EnumIter, EnumString, IntoStaticStr, Serialize, Deserialize)]
#[strum(serialize_all = "snake_case")]
pub enum IconName { AcpRegistry, AiAnthropic, /* ... */ Check, ChevronDown, /* ... */ PlayFilled, PlayOutlined, /* ... */ }

impl IconName {
    pub fn path(&self) -> Arc<str> {
        let file_stem: &'static str = self.into();
        format!("icons/{file_stem}.svg").into()
    }
}
```

[crates/icons/src/icons.rs](https://raw.githubusercontent.com/zed-industries/zed/main/crates/icons/src/icons.rs) lines 4-10 and 310-316. Two tests keep the enum and the filesystem in sync: `test_all_icons_exist` asserts every variant has a file under `assets/icons`, and `test_no_dangling_icons` asserts every file stem parses back into a variant (lines 326-357). The crate was split out of `ui` in [PR #27447](https://github.com/zed-industries/zed/pull/27447) so crates could name icons without depending on the whole UI crate; `crates/ui/src/components/icon.rs` re-exports it with `pub use icons::*;`.

### Rendering and sizes

```rust
pub enum IconSize { Indicator, XSmall, Small, #[default] Medium, XLarge, Custom(Rems) }

impl IconSize {
    pub fn rems(self) -> Rems {
        match self {
            IconSize::Indicator => rems_from_px(10_f32),
            IconSize::XSmall => rems_from_px(12_f32),
            IconSize::Small => rems_from_px(14_f32),
            IconSize::Medium => rems_from_px(16_f32),
            IconSize::XLarge => rems_from_px(48_f32),
            IconSize::Custom(size) => size,
        }
    }
}
```

The `Icon` element has three sources: `Embedded(SharedString)` for SVGs in the binary, `ExternalSvg` for SVGs on disk, and `External(Arc<Path>)` for raster icon-theme images. The embedded path renders as:

```rust
IconSource::Embedded(path) => svg()
    .with_transformation(self.transformation)
    .size(self.size)
    .flex_none()
    .path(path)
    .text_color(self.color.color(cx))
    .into_any_element(),
```

The comment on `External` explains why raster is still needed: "Currently our SVG renderer is missing support for rendering polychrome SVGs. In order to support icon themes, we render the icons as images instead." [crates/ui/src/components/icon.rs](https://raw.githubusercontent.com/zed-industries/zed/main/crates/ui/src/components/icon.rs) lines 53-79, 129-160, 215-239.

### SVG normalisation

Actual files under `assets/icons` share one shape: `width="16" height="16" viewBox="0 0 16 16" fill="none"`, a single root `<svg>` with no groups, ids or classes, and stroke-based outlines with `stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"`. Colours are hardcoded, not `currentColor`:

```xml
<svg width="16" height="16" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
<path d="M4.15186 6.47321L7.99258 10.1696L11.8483 6.47321" stroke="black" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
</svg>
```

[assets/icons/chevron_down.svg](https://raw.githubusercontent.com/zed-industries/zed/main/assets/icons/chevron_down.svg). `check.svg`, `close.svg`, `play_filled.svg` (which adds `fill="black"`) and `play_outlined.svg` follow the same pattern. `magnifying_glass.svg` uses `stroke="#DCE0E5"` and an `opacity="0.1"` circle, which survives as partial alpha because the renderer only keeps the alpha channel. The hardcoded colours are harmless for the same reason.

The stroke width was reduced from 1.5 to 1.2 in [PR #36361](https://github.com/zed-industries/zed/pull/36361): "Reducing it to 1.2 makes the UI much sharper, less burry, and more cohesive overall." The whole set was redrawn in [PR #35856](https://github.com/zed-industries/zed/pull/35856), which also created the guideline document.

### Written guidelines

[crates/icons/README.md](https://github.com/zed-industries/zed/blob/main/crates/icons/README.md) is the canonical text:

> 1. The SVG view box should be 16x16.
> 2. For outlined icons, use a 1.2px stroke width.
> 3. Not all icons are mathematically aligned; there's quite a bit of optical adjustment. However, try to keep the icon within an internal 12x12 bounding box as much as possible while ensuring proper visibility.
> 4. Use the `filled` and `outlined` terminology when introducing icons that will have these two variants.
> 5. Icons that are deeply contextual may have the feature context as their name prefix. For example, `ToolWeb`, `ReplPlay`, `DebugStepInto`, etc.
> 6. Avoid complex layer structures in the icon SVG, like clipping masks and similar elements. When the shape becomes too complex, we recommend running the SVG through SVGOMG to clean it up.

Under "Sourcing": "Most icons are created by sourcing them from Lucide. Then, they're modified, adjusted, cleaned up, and simplified depending on their use and overall fit with Zed. Sometimes, we may use other sources like Phosphor, but we also design many icons completely from scratch." Contributing: "SVG files in the assets folder follow a snake_case name format. Icons in the `icons.rs` file follow the PascalCase name format."

[CONTRIBUTING.md](https://raw.githubusercontent.com/zed-industries/zed/main/CONTRIBUTING.md) line 79 lists what is not merged: "New file icons. Zed's default icon theme consists of icons that are hand-designed to fit together in a cohesive manner, please don't submit PRs with off-the-shelf SVGs." The PR template links the icon guidelines (added in [PR #58855](https://github.com/zed-industries/zed/pull/58855)).

### Scripts and dependencies

There is no icon script under `script/` or `tooling/`; SVGOMG is a manual recommendation. The workspace pins `resvg = { version = "0.46.0", default-features = false, features = ["text", "system-fonts", "memmap-fonts", "raster-images"] }` and `usvg = { version = "0.46.0", default-features = false }`. [Cargo.toml](https://raw.githubusercontent.com/zed-industries/zed/main/Cargo.toml) lines 780-785 and 880; [crates/gpui/Cargo.toml](https://raw.githubusercontent.com/zed-industries/zed/main/crates/gpui/Cargo.toml) lines 82-83.

### Asset bundling

Assets live in `crates/assets/src/assets.rs`, wrapped in a `util::fs_embed!` macro that expands to rust-embed in release and reads from the checkout in debug:

```rust
util::fs_embed! {
    pub struct Assets,
    crate_relative = "../../assets",
    root_relative = "assets",
    include = ["fonts/**/*", "icons/**/*", "images/**/*", "themes/**/*", "sounds/**/*", "prompts/**/*", "*.md"],
    exclude = ["themes/src/*", "*.DS_Store"],
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<std::borrow::Cow<'static, [u8]>>> {
        Self::get(path).map(|f| Some(f.data)).with_context(|| format!("loading asset at path {path:?}"))
    }
    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Self::iter().filter_map(|p| if p.starts_with(path) { Some(p.into()) } else { None }).collect())
    }
}
```

[crates/assets/src/assets.rs](https://raw.githubusercontent.com/zed-industries/zed/main/crates/assets/src/assets.rs) lines 1-44; the release arm of the macro is `#[derive(RustEmbed)] #[folder = $crate_relative] #[include = ...]` in [crates/util/src/util.rs](https://raw.githubusercontent.com/zed-industries/zed/main/crates/util/src/util.rs) lines 769-775.

### Native symbols

Zed uses none. A grep of every file in `crates/gpui_macos/src` and `crates/gpui_apple/src` for `NSImage`, `SFSymbol`, `systemSymbol`, `imageNamed` returns nothing, and GitHub code search for "SF Symbol" and `systemSymbolName` in the repository returns zero results.

## 2. gpui-kit and gpui-component

gpui-kit 0.6.0 is a facade: it re-exports gpui-pre, `gpui_component` as `component` and `gpui_kit_assets` as `assets`. All icon code is in `gpui-component` 0.6.0 and `gpui-component-macros` 0.6.0. Local paths below are relative to `~/.cargo/registry/src/index.crates.io-*/`.

### The `icon_named!` macro

The macro scans a directory at compile time and emits an enum plus an `IconNamed` impl. It never embeds bytes:

```rust
for entry in dir {
    let filename = entry.file_name().to_string_lossy().to_string();
    if filename.ends_with(".svg") {
        let variant_name = pascal_case(&filename);
        let path = format!("icons/{}", filename);
        entries.push((variant_name, path));
    }
}
// ...
let expanded = quote! {
    #derive_attrs
    pub enum #enum_name { #(#variants,)* }
    impl IconNamed for #enum_name {
        fn path(self) -> SharedString {
            match self { #(Self::#variants => #paths,)* }.into()
        }
    }
};
```

`gpui-component-macros-0.6.0/src/lib.rs` lines 129-198; same file on GitHub at [crates/component-macros/src/lib.rs](https://raw.githubusercontent.com/longbridge/gpui-kit/main/crates/component-macros/src/lib.rs). Three consequences: the `icons/` prefix is hardcoded regardless of the scanned directory (line 158); the path may be a literal relative to `CARGO_MANIFEST_DIR` or a `$ENV_VAR` reference (doc comment lines 84-112); and `pascal_case` splits on `-`, `_` and `.` so `chevron-down.svg` becomes `ChevronDown`. The default enum is generated with `icon_named!(IconName, "$GPUI_KIT_DEFAULT_ICONS_DIR")` in `gpui-component-0.6.0/src/icon.rs` line 29, where the environment variable is set by gpui-component's `build.rs` from the `links = "gpui-kit-default-icons"` metadata that gpui-kit-assets' `build.rs` emits (`gpui-kit-assets-0.6.0/build.rs` lines 16-29, `gpui-component-0.6.0/build.rs` lines 19-25).

### `IconNamed` and `IconName`

```rust
pub trait IconNamed {
    /// Returns the embedded path of the icon.
    fn path(self) -> SharedString;
}

impl<T: IconNamed> From<T> for Icon {
    fn from(value: T) -> Self { Icon::build(value) }
}
```

`gpui-component-0.6.0/src/icon.rs` lines 13-22. The blanket `From` impl means any custom enum implementing `IconNamed` works wherever a component takes `impl Into<Icon>`, for example `Button::icon`.

### The `Icon` element

```rust
pub struct Icon {
    base: Svg,
    style: StyleRefinement,
    path: SharedString,
    text_color: Option<Hsla>,
    size: Option<Size>,
    rotation: Option<Radians>,
}

impl Default for Icon {
    fn default() -> Self {
        Self { base: svg().flex_none().size_4(), /* ... */ }
    }
}

impl RenderOnce for Icon {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let text_color = self.text_color.unwrap_or_else(|| window.text_style().color);
        let text_size = window.text_style().font_size.to_pixels(window.rem_size());
        let has_base_size = self.style.size.width.is_some() || self.style.size.height.is_some();
        let mut base = self.base;
        *base.style() = self.style;
        base.flex_shrink_0()
            .text_color(text_color)
            .when(!has_base_size, |this| this.size(text_size))
            .when_some(self.size, |this, size| match size {
                Size::Size(px) => this.size(px),
                Size::XSmall => this.size_3(),
                Size::Small => this.size_3p5(),
                Size::Medium => this.size_4(),
                Size::Large => this.size_6(),
            })
            .path(self.path)
    }
}
```

`gpui-component-0.6.0/src/icon.rs` lines 50-66 and 147-168. Sizes map to 12, 14, 16 and 24 px; without an explicit size the icon matches the current font size. `Icon::path` takes an asset path such as `icons/foo.svg` (lines 93-99). Rendering is always `svg().path()`; 0.6.0 has no `Icon::data`. The git main branch adds an `IconSource::Data(Arc<[u8]>)` variant and `Icon::data(&[u8])` ([crates/component/src/icon.rs on main](https://raw.githubusercontent.com/longbridge/gpui-kit/main/crates/component/src/icon.rs) lines 52-56 and 111-112), and the website documents `Icon::default().data(include_bytes!("search.svg"))` ([icon.md](https://raw.githubusercontent.com/longbridge/gpui-kit/main/website/docs/components/icon.md) lines 77-139). That API is not in the crate Cadence has.

### gpui-kit-assets

```rust
#[derive(rust_embed::RustEmbed)]
#[folder = "assets"]
#[include = "icons/**/*.svg"]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() { return Ok(None); }
        Self::get(path).map(|f| Some(f.data)).ok_or_else(|| anyhow!("could not find asset at path \"{}\"", path))
    }
    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Self::iter().filter_map(|p| p.starts_with(path).then(|| p.into())).collect())
    }
}
```

`gpui-kit-assets-0.6.0/src/native_assets.rs` lines 5-34; [GitHub copy](https://raw.githubusercontent.com/longbridge/gpui-kit/main/crates/assets/src/native_assets.rs). The crate ships 101 SVGs under `assets/icons` and nothing else. Note that `load` returns an error rather than `Ok(None)` for a missing path, so a missing icon logs on every paint. On wasm a different implementation fetches `assets/icons/*.svg` over HTTP.

The README states the set is Lucide: "The default `assets` feature bundles the Lucide icon set as `gpui-kit-assets`; pass it to the application with `gpui_kit::application().with_assets(gpui_kit::assets::Assets)`. To ship your own icons instead, leave that feature out and name the SVG files as defined in IconName." `gpui-component-0.6.0/README.md` lines 168-174; [README on GitHub](https://raw.githubusercontent.com/longbridge/gpui-kit/main/README.md). Turning off the `assets` feature on gpui-kit only stops the re-export; gpui-component still depends on gpui-kit-assets to generate `IconName` (`gpui-component-0.6.0/Cargo.toml.orig` comment: "This regular dependency propagates the canonical icon directory from the assets build script into this crate's build script through Cargo DEP metadata.").

### What the Lucide files look like

```xml
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-chevron-down">
  <path d="m6 9 6 6 6-6"/>
</svg>
```

`gpui-kit-assets-0.6.0/assets/icons/chevron-down.svg`. Of the 101 files, 100 have `viewBox="0 0 24 24"`, 96 `stroke-width="2"`, 94 `stroke="currentColor"`, 99 `fill="none"`, and 95 keep the `class="lucide lucide-*"` attribute, which shows they are unmodified Lucide exports with no optimisation pass. The exceptions are `github.svg` (filled logo), `dash.svg` (`stroke="black"`), and `resize-corner.svg` plus the four `window-*.svg` files (Sketch-style exports with `fill="#000000"` and nested `<g transform>`).

### Sync scripts

None. The full main-branch tree has no script, Makefile target or CI step that mentions icons, lucide or svgo. The repository's own `CLAUDE.md` says: "The `Icon` element does not include SVG files by default. You need to: Use Lucide or other icon libraries; Name SVG files according to the `IconName` enum definition." [CLAUDE.md](https://raw.githubusercontent.com/longbridge/gpui-kit/main/CLAUDE.md) lines 262-267.

### Combining app icons with gpui-kit's

`Application::with_assets` accepts exactly one `AssetSource` (`gpui-pre-0.3.4/src/app.rs` lines 202-209), and neither gpui nor gpui-kit ships a chaining source. The documented example registers a rust-embed struct over the app's own `./assets` with `#[include = "icons/**/*.svg"]` ([examples/app_assets/src/main.rs](https://raw.githubusercontent.com/longbridge/gpui-kit/main/examples/app_assets/src/main.rs) lines 7-29 and 45-51). Its README tells you to "just copy the svg files you want from the `assets/icons` folder in GPUI Component repo to your own assets folder" so built-in components still find `icons/check.svg`, `icons/chevron-down.svg` and so on ([examples/app_assets/README.md](https://raw.githubusercontent.com/longbridge/gpui-kit/main/examples/app_assets/README.md) lines 5-25). The alternative, used by zedis and Lumia below, is an `AssetSource` that tries the app's embed first and falls back to `gpui_kit::assets::Assets.load(path)`; the bundled struct is a unit struct with public `load` and `list`. A custom enum is either generated with `icon_named!(MyIcon, "assets/icons")` or hand-written:

```rust
impl IconNamed for IconName {
    fn path(self) -> gpui_kit::SharedString {
        match self {
            IconName::Encounters => "icons/encounters.svg",
            IconName::Monsters => "icons/monsters.svg",
        }.into()
    }
}
Button::new("my-button").icon(IconName::Spells);
```

[website/docs/components/icon.md](https://raw.githubusercontent.com/longbridge/gpui-kit/main/website/docs/components/icon.md) lines 205-246. The same page notes: "All icon paths are relative to the assets bundle root" and "Icons from Lucide.dev are designed to work well at 16px" (lines 313-319).

## 3. Other open-source GPUI apps

All repositories below were cloned and read on 2026-09-07. Every one of them ships SVGs; none uses native platform symbols for UI icons, and one uses an icon font.

| Repo | GPUI dependency | Icon set | Bundling | Render call |
| --- | --- | --- | --- | --- |
| [MatthiasGrandl/Loungy](https://github.com/MatthiasGrandl/Loungy) (archived 2025-10) | zed git | Lucide, 1421 SVGs | rust-embed + `AssetSource` | `svg().path()` |
| [hummingbird-player/hummingbird](https://github.com/hummingbird-player/hummingbird) (2026-09) | gpui-unofficial shim + gpui-ce platform | Tabler, 74 SVGs plus hand-drawn filter icons | rust-embed behind a URL-scheme `AssetSource` | `svg().path()` wrapper |
| [vicanso/zedis](https://github.com/vicanso/zedis) (2026-09) | `gpui-pre 0.3.3` + `gpui-kit 0.6.0` | Lucide: gpui-kit built-ins + 52 extra | rust-embed layered over `gpui_kit::assets::Assets` | gpui-kit `Icon::empty().path()` |
| [sonorahq/sonora](https://github.com/sonorahq/sonora) (2026-09) | nolight132/gpui fork | Four switchable packs: Lucide, Iconoir, Remix, Solar | `build.rs` generated `include_bytes!` table + `AssetSource` | `svg().path(icons::path(..))` |
| [duanebester/chat-ai](https://github.com/duanebester/chat-ai) (2025-12) | `gpui 0.2` + `gpui-component 0.5` | Lucide, 15 SVGs | rust-embed + `AssetSource` | gpui-component `Icon::empty().path()` |
| [gaauwe/fast-forward](https://github.com/gaauwe/fast-forward) (2025-12) | zed git | 2 hand-drawn 16px SVGs | rust-embed with filesystem fallback | `svg().path()` |
| [iFence/Lumia](https://github.com/iFence/Lumia) (2026-08) | zed git + gpui-component git | Lucide built-ins + 8 custom | `include_bytes!` match arms falling back to `gpui_component_assets::Assets` | gpui-component `Icon::default().path()` |
| [amtoaer/lyrune](https://github.com/amtoaer/lyrune) (2026-08) | zed git + gpui-component git | Lucide-shaped path strings in Rust | Inline SVG strings, `Image::from_bytes(ImageFormat::Svg)` cached per (icon, colour) | `img()` |
| [Ruszero01/clippi](https://github.com/Ruszero01/clippi) (2026-09) | `gpui 0.2.2` + `gpui-component 0.5` | Icon font from iconfont.cn | `include_bytes!` TTF + `add_fonts`, plus `CTFontManagerRegisterFontsForURL` on macOS | text: `.font_family("iconfont").child("\u{e638}")` |
| [66HEX/frame](https://github.com/66HEX/frame) (2026-08) | gpui-ce git | Hugeicons stroke style, inline SVG consts | Hand-written `AssetSource` matching path constants to `&str` SVGs | `svg().path()` |
| [huacnlee/gpui-calculator](https://github.com/huacnlee/gpui-calculator) (2026-01) | `gpui 0.2` + `gpui-component 0.5` | Lucide built-ins only | `with_assets(gpui_component_assets::Assets)` | gpui-component `IconName` |

Details worth keeping:

- **Loungy** derives paths from a large `enum Icon` by kebab-casing the variant name: `SharedString::from(format!("icons/{}.svg", name))` (`src/components/shared/icon.rs` line 34), then `svg().path(icon.path()).text_color(color.unwrap_or(theme.text)).size_full()` (`src/components/shared/mod.rs` line 182). Its README credits "Lucide: Amazing open source SVG icon-set".
- **hummingbird** is the closest analogue to Cadence, a music player. It keeps Tabler's MIT notice at `assets/icons/LICENSE` and its own additions under `assets/icons/LICENSE-hummingbird-icons`. Icons are addressed through constants like `pub const PLAY: &str = "!bundled:icons/player-play.svg";` that a URL-scheme dispatcher resolves (`src/ui/assets.rs` lines 22-31).
- **zedis** is on the same gpui-pre 0.3.x plus gpui-kit 0.6.0 family as Cadence. Its `AssetSource` checks `ComponentAssets::get(path)` first and then its own embed (`src/assets.rs` lines 8-31), and a test named `every_bundled_icon_loads` asserts every SVG under `assets/icons` loads. Custom icons go through `enum CustomIconName` with `impl From<CustomIconName> for Icon { Icon::empty().path(val.path()) }`.
- **sonora** lets the user switch icon packs at runtime; a `build.rs` walks `assets/icons/*/` and emits one `include_bytes!` per file into a `PACKS` table, and every pack keeps its own license file beside its files.
- **clippi** documents the pitfall of icon fonts in `scripts/patch-iconfont.py`: "GPUI's macOS and Linux text systems require every font to have a glyph for the letter 'm' ... Icon-only fonts that only have glyphs in the Private Use Area are silently dropped, causing all icons to render as tofu."
- **lyrune** bakes the tint into the SVG string and renders with `img()`, so each (icon, colour) pair is a separate cached image; it shows what the alternative to `text_color` masking costs.

Not found or not applicable: `longbridge/gpui-app-template` (404), `polachok/helix-gpui` (exists, no icons), `zortax/gpui-terminal` (library, no icons), and the names "gpui-todo", "zeta", "kiro", "Moxide", "Reqable", "Kaku", "OpenCode desktop", "Dnote" did not resolve to GPUI repositories.

## 4. Runtime SF Symbols in GPUI

Two codebases render SF Symbols at runtime for GPUI. GitHub code search for `systemSymbolName language:Rust` found roughly 30 files, all in Tauri, cidre or objc2 projects, none GPUI; the search hit the API rate limit, so the list may be incomplete.

### AprilNEA/gpui-symbols (Cadence's current dependency)

- Repository [github.com/AprilNEA/gpui-symbols](https://github.com/AprilNEA/gpui-symbols), 18 stars, last commit 2026-01-21 ("feat: preserve aspect ratio for non-square symbols"). crates.io latest is 0.6.1, published 2026-01-21; all eleven versions from 0.1.0 to 0.6.1 were published within 2026-01-20 and 2026-01-21, nothing since ([crates.io](https://crates.io/crates/gpui-symbols)). The registry copy at `gpui-symbols-0.6.1/src` is byte-identical to the repository's `src`.
- GPUI support: the only GPUI dependency is `gpui = { version = "0.2.2", default-features = false, optional = true }` behind the `gpui`, `component` and `cache` features (`gpui-symbols-0.6.1/Cargo.toml` lines 30-61). There is no gpui-pre, gpui-kit or gpui-component feature. Cadence sidesteps the version conflict with `gpui-symbols = { version = "0.6.1", default-features = false }` (`Cargo.toml` line 18), which leaves only `SfSymbol::render_rgba()` available, and Cadence rebuilds the GPUI element itself in `src/app/sf_icon.rs`. Another downstream, tschk/crepuscularity, records the same conflict: "gpui-symbols 0.6 depends on crates.io gpui 0.2.2, which conflicts with the gpui-ce fork."
- Rasterisation: `NSImage imageWithSystemSymbolName:accessibilityDescription:`, then `NSImageSymbolConfiguration configurationWithPointSize:weight:scale:` combined with hierarchical, monochrome or multicolor configuration, `imageWithSymbolConfiguration:`, and `drawInRect:fromRect:operation:fraction:` into an `NSBitmapImageRep` of `ceil(size * scale)` pixels (`src/platform/macos.rs` lines 64-210, legacy `objc 0.2` `msg_send!`).
- Scale: `SfSymbol::new` hardcodes `scale: 2.0` with the comment "default: 2.0 for Retina" (`src/symbol.rs` lines 216 and 230-233), and nothing reads the window's scale factor. Cadence's `sf_icon.rs` also never calls `.scale()`, so symbols are always rasterised at 2x regardless of display.
- Tint: the colour drops alpha (`color_u32 >> 8`, "ignore alpha for now") and becomes an `NSColor` applied through the symbol configuration. Cadence mirrors this in `rgb_color()`.
- Handoff: RGBA is swapped to BGRA "for GPUI's Metal renderer", wrapped in `RenderImage`, and drawn with `img(ImageSource::Render(image))` (`src/symbol.rs` lines 369-376, `src/icon.rs` line 261). Cadence's `sf_icon.rs` reproduces this pipeline and its `(name, size, colour, weight, scale, mode)` cache.
- Other users found by code search: anomalyco/hex (with a Linux fallback that inlines Heroicons SVGs) and realartists-gitmo/flowstate (SF Symbols on macOS, gpui-component `IconName` elsewhere).

### tiborgats/native-theme and native-theme-gpui

- [github.com/tiborgats/native-theme](https://github.com/tiborgats/native-theme), last commit 2026-09-07; crates.io `native-theme 0.5.8` and `native-theme-gpui 0.5.8`, both published 2026-09-07. The gpui connector targets Cadence's family: `gpui = { package = "gpui-pre", version = "0.3.3" }`, `gpui-component = "0.6.0"`, `gpui-base = "0.6.0"` (`connectors/native-theme-gpui/Cargo.toml` lines 38-41).
- Rasterisation uses objc2: `NSImage::imageWithSystemSymbolName_accessibilityDescription`, `NSImageSymbolConfiguration::configurationWithPointSize_weight_scale(point_size, NSFontWeightRegular, NSImageSymbolScale::Medium)`, `CGImageForProposedRect_context_hints` ("on Retina displays will be at the full pixel resolution"), then a `CGBitmapContext` draw and `unpremultiply_alpha` (`native-theme/src/sficons.rs` lines 29-109). Point size is a fixed `DEFAULT_ICON_SIZE: u32 = 24`; weight and scale are not configurable through the public loader.
- Handoff encodes the RGBA to BMP and uses `Image::from_bytes(ImageFormat::Bmp, bmp)`, with a doc comment explaining it "Works around a gpui bug where `ImageFormat::Svg` in `Image::to_image_data` skips the RGBA to BGRA pixel conversion" (`connectors/native-theme-gpui/src/icons.rs` around line 1149). Size and colour parameters apply only to SVG data; SF Symbol bitmaps pass through untinted.
- It is a theme-role loader (a fixed mapping of UI roles to symbols), not a general "any symbol name at any weight" component.

## 5. How gpui's `svg()` element renders

Read from `gpui-pre-0.3.4` in the local cargo registry.

- **Renderer.** `Cargo.toml` lines 425-431 and 500-502 pin `resvg = "0.46.0"` with features `text`, `system-fonts`, `memmap-fonts`, `raster-images` and `usvg = "0.46.0"`, both with default features off. `svg_renderer.rs` parses with `usvg::Tree::from_data(bytes, &self.usvg_options)` (line 267) and rasterises with `resvg::render(tree, transform, &mut pixmap.as_mut())` (line 306) into a `tiny_skia::Pixmap`.
- **Mask behaviour.** `render_alpha_mask` throws away colour: "Convert the pixmap's pixels into an alpha mask" followed by `pixmap.pixels().iter().map(|p| p.alpha()).collect()` (lines 233-254). `Window::paint_svg` then inserts a `MonochromeSprite` with `color: color.opacity(element_opacity)` (`window.rs` lines 4605-4665). So the SVG's own fill, stroke and `currentColor` are irrelevant; only coverage matters, and partial opacity in the file becomes partial alpha. Polychrome SVGs cannot be rendered through this path, which is why Zed falls back to `img()` for icon themes.
- **Colour comes from the style.** In `elements/svg.rs` the paint closure only draws when `style.text.color` is set: `if let Some((path, color)) = self.path.as_ref().zip(style.text.color) { window.paint_svg(bounds, path.clone(), None, transformation, color, cx) }` (lines 148-187). An `svg()` without a text colour in its style chain paints nothing.
- **Three sources.** `Svg` holds `path` (asset path resolved through `AssetSource::load`), `external_path` (a filesystem path read asynchronously through the `SvgAsset` asset type with `fs::read`), and `data` (raw bytes; `Svg::data` hashes the bytes into a synthetic cache key `__binary_svg__{hash}`) (`elements/svg.rs` lines 16-62 and 285-303). The `data` path exists in gpui-pre 0.3.4 even though gpui-component 0.6.0 does not expose it.
- **Size handling.** The element's layout bounds decide the raster size: `paint_svg` asks for `bounds.size * SMOOTH_SVG_SCALE_FACTOR` device pixels, where `pub const SMOOTH_SVG_SCALE_FACTOR: f32 = 2.;` "When rendering SVGs, we render them at twice the size to get a higher-quality result" (`svg_renderer.rs` line 81, `window.rs` line 4622). `rasterize_tree` uses `SvgSize::Size`, which scales the tree by `requested_width / svg_size.width()` and keeps the aspect ratio (lines 272-284), then the sprite is centred inside the element bounds and clamped to 8192 px ("Cap the size of the rendered pixmap to avoid texture allocation panics", line 275). The viewBox therefore only defines proportions; a 24x24 Lucide file and a 16x16 Zed file both fill whatever `.size_4()` gives them. Because `bounds` are already in device pixels after `snap_bounds`, the raster respects the window's scale factor; there is no hardcoded Retina assumption as in gpui-symbols.
- **Caching.** Rasters are cached in the sprite atlas keyed by `RenderSvgParams { path, size }` (`window.rs` lines 4620-4636), so one icon at one size is rasterised once per atlas, and tint changes are free because colour is applied in the shader.
- **gpui-kit sizing.** `Icon` starts from `svg().flex_none().size_4()` and maps `Size::XSmall` to `size_3()`, `Small` to `size_3p5()`, `Medium` to `size_4()`, `Large` to `size_6()`; with no size set it uses the current font size (section 2).

## 6. Exporting SF Symbols to SVG

### The official flow

Apple documents two commands in the SF Symbols app. For editing: "To create an SVG file for your custom symbol, export a symbol template file for design customization by selecting the symbol and choosing File > Export Template." For distribution to Xcode: "There are two options to consider when distributing a symbol. Choose File > Export Symbol to begin." [Creating custom symbol images for your app](https://developer.apple.com/documentation/uikit/creating-custom-symbol-images-for-your-app).

Static versus variable: "When exporting a template, you choose between static or variable. Use a static setup if you're targeting a particular weight and scale, or only plan to design one or two variants of your symbol. The setup contains 27 sets of paths and one set of explicit margins. A variable template setup contains three sets of paths and three sets of margins." Same page.

### Template structure

The template is one large SVG (`width="3300" height="2200"`, generator comment `Apple Native CoreSVG`) with three top-level groups:

- `<g id="Notes">` holds `<text id="template-version">Template v.3.0</text>` and an `artboard` layer. "The `template-version` layer contains a required version string that indicates the template format version, so you must not remove it; otherwise, SF Symbols can't read the file."
- `<g id="Guides">` holds `Baseline-S`, `Capline-S`, `Baseline-M`, `Capline-M`, `Baseline-L`, `Capline-L` lines, an outline capital A per scale as the reference glyph, and margin lines named `left-margin-<variant>` and `right-margin-<variant>`, for example `<line id="left-margin-Regular-S" style="fill:none;stroke:#00AEEF;stroke-width:0.5;opacity:1.0;" .../>`.
- `<g id="Symbols">` holds "up to 27 sublayers, each representing a symbol image variant. Identifiers of symbol variants have the form `<weight>-<scale>`, where weight corresponds to a weight of the San Francisco system font and S, M, or L matches the small, medium, or large symbol scale." Example: `<g id="Regular-M" transform="matrix(1 0 0 1 2855.62 1556)">`. The nine weights are Ultralight, Thin, Light, Regular, Medium, Semibold, Bold, Heavy, Black. Scale factors relative to M are S 0.783 and L 1.29. "Beginning with template version 3, SF Symbols introduces vector interpolation ... By using three sources, `Ultralight-S`, `Regular-S`, and `Black-S`, SF Symbol can dynamically generate the full range of weights and scales you don't specify."

All quotes from the same Apple page. Template versions: version 2 "removes annotation data and explicit margins ... only contains monochrome"; version 3 "embeds all of your multicolor and hierarchical data annotations, as well as any custom margins"; version 4 "embeds your variable color annotations". Apple's page stops at version 4, and the WWDC23 to WWDC25 transcripts do not name a 5.0 or 6.0 template; WWDC24 only says "re-export the symbols from the SF Symbols 6 app, and import them into Xcode 16" ([WWDC24 10188](https://developer.apple.com/videos/play/wwdc2024/10188/)).

WWDC transcripts confirm the layer naming matters: "Each one of these symbols is in its own layer with a unique descriptive identifier. And theses layer names are vital to the integrity of the file." [WWDC20 10207](https://developer.apple.com/videos/play/wwdc2020/10207/). "This will export a 3.0 template in monochrome so that I can customize it. ... In 3.0, the left-margin and right-margin guidelines have more explicit names, indicating the design variant that they correspond to." [WWDC21 10250](https://developer.apple.com/videos/play/wwdc2021/10250/). Apple also warns that none of the exports are meant as source artifacts for other tools: "None of the versions is a source artifact for editing. Current design tools may not be compatible with the embedded annotation data."

### Extracting one variant

Mechanically, extracting a single icon means taking the child paths of the `<g id="Semibold-L">` (or whichever) group out of `<g id="Symbols">`, dropping the group's translation `transform`, and writing a new `<svg>` whose viewBox is the path bounding box or the margin lines for that variant. Apple provides no tool for this. Open-source tools take a different route and read the glyph outlines from the SF Pro font instead of the template:

| Tool | Status | Method | License note in README |
| --- | --- | --- | --- |
| [davedelong/sfsymbols](https://github.com/davedelong/sfsymbols) | Archived, "MAINTAINER WANTED", last commit 2021-01 | Locates `SF Symbols.app`, reads the bundled font, exports `svg`, `png`, `pdf`, `iconset` per `--font-weight` and `--symbol-size` | "It is your responsibility to make sure you are following the terms and conditions of using Apple's symbols." |
| [yapstudios/sfsym](https://github.com/yapstudios/sfsym) | Active, v0.2.11 2026-05, MIT | `NSImage(systemSymbolName:)` with a symbol configuration, reads the private `_vectorGlyph` of `NSSymbolImageRep`, draws into a `CGPDFContext`, then rewrites PDF operators as SVG `d` commands | "The SF Symbols License permits their use only in artwork and mockups for apps that run on Apple platforms. `sfsym` is a tool; the restriction applies to what you ship with the output it produces." |
| [brendanballon/sfsymbols-svg](https://github.com/brendanballon/sfsymbols-svg) | Active, 2026-07, no license file | Swift script reads `symbol_names.txt`, `symbol_chars.txt` and `SF-Pro.ttf` from the app bundle and uses `CTFontCreatePathForGlyph`; publishes 6,404 pre-rendered SVGs | None |
| [MoOx/sf-symbols-svg](https://github.com/MoOx/sf-symbols-svg) (npm) | Active, 8.0.0-beta, MIT | Renders from the installed SF Pro Text fonts by codepoint, `--weight`, `--size`, `--padding` | "For legal reasons, this repository does not include the SF Pro Text font files." |
| [swhitty/SwiftDraw](https://github.com/swhitty/SwiftDraw), [jaywcjlove/create-custom-symbols](https://github.com/jaywcjlove/create-custom-symbols) | Active | Opposite direction: turn your SVG into an SF Symbols template | n/a |

No `xcrun` tool exports symbols (`actool --help` has no symbol option). The SF Symbols 7 installer writes `SF-Pro.ttf`, `SF-Pro-Italic.ttf`, `SF-Compact.ttf` and `SF-Compact-Italic.ttf` into `/Library/Fonts`, which is what the font-based tools read; Apple's documentation says nothing about extracting glyphs from those files, but the license does (next section).

## 7. SF Symbols license

### Where the text is

Apple does not publish the SF Symbols license on a web page. `https://www.apple.com/legal/sla/docs/SFSymbols.pdf` returns 404 and [developer.apple.com/sf-symbols](https://developer.apple.com/sf-symbols/) has no license link. The text is inside the installer: `SF-Symbols-7.dmg` > `SF Symbols.pkg` > `Resources/English.lproj/License.rtf` (agreement identifiers EA1662 for the app, EA1644 for the SF font, dated 9/6/2019). The quotes below were extracted from that file on 2026-09-07. The download is `https://devimages-cdn.apple.com/design/resources/download/SF-Symbols-7.dmg`.

### Software License Agreement for the Apple SF Symbols App (EA1662)

Title line: "SOFTWARE LICENSE AGREEMENT FOR THE APPLE SF SYMBOLS APP / For iOS, iPadOS, macOS, tvOS and watchOS application uses only".

Important note: "IMPORTANT NOTE: THE APPLE SOFTWARE IS TO BE USED SOLELY FOR CREATING USER INTERFACES TO BE USED IN SOFTWARE PRODUCTS RUNNING ON APPLE'S iOS, iPadOS, macOS, tvOS OR watchOS OPERATING SYSTEMS, AS APPLICABLE, AND IS SUBJECT TO THE SPECIFIC USE RESTRICTIONS SET FORTH HEREIN."

2A, the grant: "you are granted a limited, non-transferable, non-exclusive license to install and use a reasonable number of copies of the Apple Software internally within your company or organization for the limited purpose of using Symbols from the Apple Software to create mock-ups of user interfaces to be used in software products running on the applicable Apple Platforms (as defined below)." And: "Your use of Symbols obtained from Apple's SF Font is limited to creating mock-ups of user interfaces for software products running on Apple's iOS, iPadOS, macOS and tvOS operating systems".

2B, templates, the clause that governs exported SVGs: "The Apple Software may enable the user to generate editable templates of certain Symbols in an svg file format ("Template(s)"). ... you are granted a limited, non-transferable, non-exclusive license ... for the limited purposes of: (i) generating Templates using the Apple Software; (ii) modifying or having modifications made to Templates generated by the Apple Software; (iii) using modified Templates to create mock-ups of user interfaces to be used in software products running on an Apple Platform; and (iv) incorporating and distributing such modified Templates in your software products running on the applicable Apple Platforms, provided that such modified Templates are incorporated into your software products exclusively through the use of Apple's Xcode application."

2C: "Apple shall retain all rights, title, and interest in and to Templates generated by the Apple Software."

2D, the catch-all: "Except as expressly provided herein, Symbols and Templates may not otherwise be used, extracted, copied, modified, distributed, embedded or repackaged as content, images, samples, clip art, or similar assets."

2E, trademarks and embedding: "You agree that you shall not use or incorporate the Symbols or any substantially or confusingly similar images into app icons, logos or make any other trademark use of the Symbols. Apple reserves the right to review and, in its sole discretion, require modification or discontinuance of use of any Symbol used in violation of the foregoing restrictions, and you agree to promptly comply with any such request." And: "The grants set forth in this License do not permit you to, and you agree not to, install, use or run the Apple Software, Symbols or Templates for the purpose of creating mock-ups of user interfaces to be used in software products running on any non-Apple operating system or to enable others to do so. You may not embed the Apple Software in any software programs or other products."

Section 4 ends the license "(b) if you are no longer a registered Apple Developer".

Two wording corrections against common paraphrases: visionOS does not appear anywhere in EA1662 (a developer raised this in [forum thread 739523](https://developer.apple.com/forums/thread/739523), and an Apple "Graphics and Games Engineer" replied only "We will pass this along internally and see if we can get additional clarification on this"). The phrase "You may not embed the Apple Font or the SF Symbols" does not appear; the app license says "You may not embed the Apple Software" and the font license (EA1644, bundled in the same RTF) says "You may not embed the Apple Font in any software programs or other products."

### Human Interface Guidelines

[HIG, SF Symbols](https://developer.apple.com/design/human-interface-guidelines/sf-symbols): "Be sure to understand the terms and conditions for using SF Symbols, including the prohibition against using symbols, or images that are confusingly similar, in app icons, logos, or any other trademarked use." And: "SF Symbols includes copyrighted symbols that depict Apple products and features. You can display these symbols in your app, but you can't customize them. To help you identify a noncustomizable symbol, the SF Symbols app badges it with an Info icon; to help you use the symbol correctly, the inspector pane describes its usage restrictions." The older sentence "All SF Symbols shall be considered to be system-provided images as defined in the Xcode and Apple SDKs license agreements" is quoted in several forum threads (661582, 706089, 724523, 728489) but is no longer on the current page and could not be verified against an archive.

### Xcode and Apple SDKs Agreement

[xcode.pdf](https://www.apple.com/legal/sla/docs/xcode.pdf) (EA2002, 06/08/2026), section 2.10: "System-Provided Images. The system-provided assets (e.g., images, symbols) owned by Apple and documented as such in Apple's Human Interface Guidelines for iOS, watchOS, iPadOS, tvOS, macOS, or visionOS ("System-Provided Images") are licensed to You solely for the purpose of developing Applications for Apple-branded products that run on the system for which the image was provided. You agree that you shall not use or incorporate the System-Provided Images or any substantially or confusingly similar images into app icons, logos or make any other trademark use of the System-Provided Images. Your use of the System-Provided Images shall also be subject to any specific use restrictions with respect thereto as set forth in the Apple Software or Apple's Human Interface Guidelines. ... Upon termination of this Agreement, You may continue to distribute the System-Provided Images as used within Applications You developed using the Apple Software."

The Apple Developer Program License Agreement contains no "system-provided images" or "SF Symbols" clause; it defers to the Xcode agreement. No Apple staff statement was found on whether exported SF Symbol SVGs may be shipped inside a macOS app bundle; the controlling text is 2B(iv) above.

### Reading for a GPUI app

- Rendering SF Symbols at runtime through AppKit (`NSImage imageWithSystemSymbolName:`) in a macOS-only app is the use case section 2.10 covers: system-provided images, used in an application for the platform that provides them. Cadence today does this.
- Shipping SVGs exported from the SF Symbols app inside a Rust binary fails 2B(iv), because the templates are not "incorporated into your software products exclusively through the use of Apple's Xcode application", and falls under 2D's prohibition on templates being "extracted, copied, modified, distributed, embedded or repackaged". Outlines pulled from `SF-Pro.ttf` by the third-party tools are Symbols "obtained from Apple's SF Font" and carry the same "mock-ups of user interfaces" limitation plus the font license's embedding ban.
- Either way, symbols may not appear in the app icon, logo or any trademark use (2E, HIG, xcode.pdf 2.10).

## Implications for Cadence

Cadence today: `src/app/sf_icon.rs` wraps `gpui_symbols::SfSymbol::render_rgba()` into a `RenderImage` drawn with `img()`, with a process-wide cache keyed on name, size, colour, weight, scale and mode. `src/app/components.rs` line 59 fixes the house style at `SymbolWeight::Semibold`, `SymbolScale::Large`, `RenderingMode::Monochrome`. Around twenty distinct symbol names are used (`play.fill`, `pause.fill`, `backward.end.fill`, `forward.end.fill`, `heart.fill`, `star.fill`, `pin.fill`, `music.note`, `music.note.list`, `list.bullet`, `person.fill`, `clock.fill`, `chevron.left`, `arrow.up.right`, `speaker.wave.2.fill`, `speaker.slash.fill`, `square.on.square`, `circle.lefthalf.filled`, `sun.max`, `rectangle.portrait.and.arrow.right`, `xmark`). `src/app/bootstrap.rs` line 21 already registers `gpui_kit::assets::Assets`, so gpui-kit's built-in Lucide `IconName` is available now.

### Option A: keep runtime SF Symbols

Pros: zero asset maintenance, exact match with macOS system UI, free access to every weight and both filled and outlined variants, all nine weights interpolated by AppKit, and the licensing position is the one the Xcode agreement anticipates (section 7). Cadence's own `sf_icon.rs` already insulates it from gpui-symbols' gpui 0.2.2 dependency.

Cons: gpui-symbols is effectively unmaintained (eleven releases in two days in January 2026, nothing since; section 4) and Cadence only uses its `render_rgba` function, so the dependency buys little beyond the `objc 0.2` AppKit calls. Rasterisation ignores the real window scale factor (hardcoded 2x), tint is baked into the bitmap so each colour is a new texture, alpha is dropped, and every icon goes through `img()` rather than the SVG atlas with shader tinting. It is macOS-only by construction, which is fine for Cadence's stated scope. If this option is kept, the sensible follow-up is to inline the roughly 150 lines of AppKit code into Cadence (or move to objc2 as native-theme does), read `window.scale_factor()`, and drop the crate.

### Option B: export SF Symbols to SVG and bundle them

Pros: would let Cadence use gpui-kit's `Icon` and the alpha-mask atlas while keeping the SF look.

Cons: not permitted. License 2B(iv) allows distributing exported templates only when "incorporated into your software products exclusively through the use of Apple's Xcode application", and 2D forbids symbols being "extracted, copied, modified, distributed, embedded or repackaged" otherwise. Font-derived outlines fall under the same "mock-ups" limitation and the font license's "You may not embed the Apple Font in any software programs or other products." The tooling that exists is either archived (davedelong/sfsymbols) or explicitly disclaims shipping the output outside Apple platforms (sfsym), and Apple says the template "is not a source artifact for editing" (section 6). This option should be dropped.

### Option C: adopt Lucide from gpui-kit-assets

Pros: already compiled into Cadence through `gpui_kit::assets::Assets`; `IconName` has direct matches for most of the current set (`play`, `pause`, `heart`, `star`, `user`, `search`, `chevron-left`, `external-link`, `close`, `sun`, `moon`, `copy`, `settings`, `menu`; section 2 lists all 101). Icons render through `svg().path()` into the sprite atlas, are tinted by `text_color` in the shader, respect the window scale factor, and cost one raster per size rather than per colour (section 5). Every third-party gpui-kit app surveyed does this (section 3), and the license is ISC. Adding icons is copying a Lucide SVG into `assets/icons/` and either extending an `icon_named!` enum or a hand-written `IconNamed` impl, with a zedis-style test that every bundled file loads.

Cons: gpui-kit-assets is missing several Cadence icons (`skip-back`, `skip-forward`, `volume-2`, `volume-x`, `list-music`, `music`, `pin`, `clock`, `log-out`, `contrast`, `list`) so Cadence must register its own `AssetSource` that falls back to `gpui_kit::assets::Assets` (the pattern used by zedis and Lumia, section 2 and 3). Lucide is a 2px-stroke 24-grid outline set; filled variants (`play.fill`, `heart.fill`, `star.fill`) need Lucide's `fill="currentColor"` treatment or a hand edit, and the visual language departs from macOS system icons. gpui-kit's own bundle is unnormalised (class attributes, a few Sketch exports), so Cadence should copy and clean the files it uses rather than rely on the bundle's contents staying stable.

### Option D: a custom drawn set

Pros: full control over weight, optical size and fill versus outline, in line with Zed's approach, where most icons start as Lucide and are then redrawn on a 16x16 grid with 1.2px round-capped strokes and a 12x12 safe area (section 1). Can start from Lucide or Phosphor (Zed's stated sources) and match Cadence's Semibold SF look more closely than stock Lucide.

Cons: design effort and ongoing maintenance for around twenty icons plus any future ones; Zed's pipeline is manual (no script, SVGOMG by hand). Needs the same `AssetSource` fallback as option C because gpui-kit's built-in components still request `icons/check.svg` and friends.

### Recommendation

Drop option B on license grounds. Between A and C or D, the decisive factors are maintenance of the renderer and correctness of scale handling. If Cadence wants to stay with SF Symbols, it should own the AppKit rendering code itself rather than depend on gpui-symbols. If Cadence wants to move to SVG, the practical path is C with a small D layer: register a Cadence `AssetSource` that embeds `assets/icons/**/*.svg` with rust-embed and falls back to `gpui_kit::assets::Assets`, generate a `CadenceIcon` enum with `icon_named!`, start from Lucide files copied and cleaned to `fill="none"`, `stroke="currentColor"` or `fill="currentColor"`, no `class` attributes, and hand-draw the filled playback glyphs on a 16 or 24 grid following Zed's rules. Add a test that every enum variant loads through the asset source, as both Zed and zedis do. Cadence's `sf_icon.rs` and the gpui-symbols dependency can then be removed.
