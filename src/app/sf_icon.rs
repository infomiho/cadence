//! SF Symbols as GPUI elements.
//!
//! `gpui-symbols` renders symbols to pixels through CoreGraphics; turning those
//! pixels into a `RenderImage` lives here so the crate does not have to track
//! gpui-kit's GPUI family.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use gpui_kit::{
    App, Empty, Hsla, ImageSource, IntoElement, Pixels, RenderImage, RenderOnce, Rgba,
    SharedString, Styled, Window, img, px, rgb,
};
use gpui_symbols::{RenderingMode, SfSymbol, SymbolScale, SymbolWeight};
use image::{Frame, RgbaImage};
use smallvec::smallvec;

/// A rendered symbol with its pixel dimensions, which need not be square.
type CachedSymbol = (Arc<RenderImage>, u32, u32);

#[derive(Clone, PartialEq, Eq, Hash)]
struct SymbolKey {
    name: SharedString,
    size_bits: u32,
    color: u32,
    weight: u8,
    symbol_scale: u8,
    rendering_mode: u8,
}

static SYMBOL_CACHE: OnceLock<Mutex<HashMap<SymbolKey, CachedSymbol>>> = OnceLock::new();

fn symbol_cache() -> &'static Mutex<HashMap<SymbolKey, CachedSymbol>> {
    SYMBOL_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// An SF Symbol drawn at a size and colour, sized to preserve its aspect ratio.
#[derive(Clone, IntoElement)]
pub(super) struct Icon {
    name: SharedString,
    size: Pixels,
    color: Hsla,
    weight: SymbolWeight,
    symbol_scale: SymbolScale,
    rendering_mode: RenderingMode,
}

impl Icon {
    pub(super) fn new(name: impl Into<SharedString>) -> Self {
        Self {
            name: name.into(),
            size: px(16.),
            color: gpui_kit::black(),
            weight: SymbolWeight::default(),
            symbol_scale: SymbolScale::default(),
            rendering_mode: RenderingMode::default(),
        }
    }

    pub(super) fn with_size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    pub(super) fn text_color(mut self, hex: u32) -> Self {
        self.color = rgb(hex).into();
        self
    }

    pub(super) fn weight(mut self, weight: SymbolWeight) -> Self {
        self.weight = weight;
        self
    }

    pub(super) fn symbol_scale(mut self, scale: SymbolScale) -> Self {
        self.symbol_scale = scale;
        self
    }

    pub(super) fn rendering_mode(mut self, mode: RenderingMode) -> Self {
        self.rendering_mode = mode;
        self
    }

    fn rgb_color(&self) -> u32 {
        let rgba: Rgba = self.color.into();
        u32::from(rgba) >> 8
    }

    fn cache_key(&self) -> SymbolKey {
        SymbolKey {
            name: self.name.clone(),
            size_bits: f32::from(self.size).to_bits(),
            color: self.rgb_color(),
            weight: self.weight as u8,
            symbol_scale: self.symbol_scale as u8,
            rendering_mode: self.rendering_mode as u8,
        }
    }

    fn render_image(&self) -> Option<CachedSymbol> {
        let key = self.cache_key();
        if let Some(cached) = symbol_cache().lock().ok()?.get(&key) {
            return Some(cached.clone());
        }
        let (width, height, mut pixels) = SfSymbol::new(self.name.as_ref())
            .size(f32::from(self.size))
            .color(self.rgb_color())
            .weight(self.weight)
            .symbol_scale(self.symbol_scale)
            .rendering_mode(self.rendering_mode)
            .render_rgba()?;
        rgba_to_bgra(&mut pixels);
        let frame = Frame::new(RgbaImage::from_raw(width, height, pixels)?);
        let image = Arc::new(RenderImage::new(smallvec![frame]));
        let rendered = (image, width, height);
        if let Ok(mut cache) = symbol_cache().lock() {
            cache.insert(key, rendered.clone());
        }
        Some(rendered)
    }
}

/// GPUI's Metal renderer expects BGRA pixels.
fn rgba_to_bgra(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
}

/// Fits a `width` x `height` image inside a square of `target` on its longer side.
fn fit_to_square(width: u32, height: u32, target: f32) -> (f32, f32) {
    let aspect_ratio = width as f32 / height as f32;
    if aspect_ratio >= 1. {
        (target, target / aspect_ratio)
    } else {
        (target * aspect_ratio, target)
    }
}

impl RenderOnce for Icon {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let Some((image, width, height)) = self.render_image() else {
            return Empty.into_any_element();
        };
        let (display_width, display_height) = fit_to_square(width, height, f32::from(self.size));
        img(ImageSource::Render(image))
            .w(px(display_width))
            .h(px(display_height))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::fit_to_square;

    #[test]
    fn wide_symbols_are_bounded_by_width() {
        assert_eq!(fit_to_square(40, 20, 16.), (16., 8.));
    }

    #[test]
    fn tall_symbols_are_bounded_by_height() {
        assert_eq!(fit_to_square(10, 20, 16.), (8., 16.));
    }

    #[test]
    fn square_symbols_fill_the_box() {
        assert_eq!(fit_to_square(20, 20, 16.), (16., 16.));
    }
}
