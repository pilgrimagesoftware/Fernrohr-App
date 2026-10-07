//! A test text system that records which font family each laid-out line was
//! shaped in, so a render test can assert a type role on real rendered text.
//!
//! GPUI keeps an element's resolved text style to itself, but every line it
//! draws goes through `PlatformTextSystem::layout_line` with the font IDs the
//! text system handed out. This wraps GPUI's `NoopTextSystem`, which shapes
//! every glyph the same regardless of font. It gives each family its own
//! `FontId`, so a line's runs say which family they were drawn in.

use gpui_kit::{
    Bounds, DevicePixels, Font, FontId, FontMetrics, FontRun, GlyphId, LineLayout, NoopTextSystem,
    Pixels, PlatformTextSystem, RenderGlyphParams, Result, SharedString, Size, TestAppContext,
    TestDispatcher, TextRenderingMode,
};
use parking_lot::Mutex;
use std::borrow::Cow;
use std::sync::Arc;

pub(crate) struct RecordingTextSystem {
    noop: NoopTextSystem,
    /// `families[i]` is the family `FontId(i)` was handed out for.
    families: Mutex<Vec<SharedString>>,
    /// Every line laid out, with the families of its runs, in order.
    lines: Mutex<Vec<(String, Vec<SharedString>)>>,
    /// Every font file handed to `add_fonts`, in order.
    added: Mutex<Vec<Vec<u8>>>,
}

impl Default for RecordingTextSystem {
    fn default() -> Self {
        Self {
            noop: NoopTextSystem::new(),
            families: Mutex::default(),
            lines: Mutex::default(),
            added: Mutex::default(),
        }
    }
}

impl RecordingTextSystem {
    /// Every font file registered so far.
    pub(crate) fn added_fonts(&self) -> Vec<Vec<u8>> {
        self.added.lock().clone()
    }

    /// Every line laid out so far, with the families of its runs.
    pub(crate) fn lines(&self) -> Vec<(String, Vec<SharedString>)> {
        self.lines.lock().clone()
    }

    /// The families `text` was drawn in, from the last line laid out that is
    /// exactly `text` - else the last that contains it. `None` if no line
    /// contains it.
    pub(crate) fn families_of(&self, text: &str) -> Option<Vec<SharedString>> {
        let lines = self.lines.lock();
        // A line that is exactly `text` first - the label itself, rather than
        // a longer line that happens to mention it, like an error naming the
        // context a context bar shows.
        lines
            .iter()
            .rev()
            .find(|(line, _)| line == text)
            .or_else(|| lines.iter().rev().find(|(line, _)| line.contains(text)))
            .map(|(_, families)| families.clone())
    }

    /// The single family `text` was drawn in, panicking with every recorded
    /// line if it was never drawn or was drawn in a mix of families.
    pub(crate) fn family_of(&self, text: &str) -> SharedString {
        let families = self.families_of(text).unwrap_or_else(|| {
            let drawn: Vec<String> = self.lines().into_iter().map(|(line, _)| line).collect();
            panic!("{text:?} was never laid out; lines drawn: {drawn:?}")
        });
        match families.as_slice() {
            [family] => family.clone(),
            _ => panic!("{text:?} was drawn in {families:?}, not one family"),
        }
    }
}

impl PlatformTextSystem for RecordingTextSystem {
    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        self.added
            .lock()
            .extend(fonts.iter().map(|font| font.to_vec()));
        self.noop.add_fonts(fonts)
    }

    fn all_font_names(&self) -> Vec<String> {
        self.noop.all_font_names()
    }

    fn font_id(&self, descriptor: &Font) -> Result<FontId> {
        let mut families = self.families.lock();
        let index = families
            .iter()
            .position(|family| *family == descriptor.family)
            .unwrap_or_else(|| {
                families.push(descriptor.family.clone());
                families.len() - 1
            });
        Ok(FontId(index))
    }

    fn font_metrics(&self, font_id: FontId) -> FontMetrics {
        self.noop.font_metrics(font_id)
    }

    fn typographic_bounds(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Bounds<f32>> {
        self.noop.typographic_bounds(font_id, glyph_id)
    }

    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Size<f32>> {
        self.noop.advance(font_id, glyph_id)
    }

    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        self.noop.glyph_for_char(font_id, ch)
    }

    fn glyph_raster_bounds(&self, params: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> {
        self.noop.glyph_raster_bounds(params)
    }

    fn rasterize_glyph(
        &self,
        params: &RenderGlyphParams,
        raster_bounds: Bounds<DevicePixels>,
    ) -> Result<(Size<DevicePixels>, Vec<u8>)> {
        self.noop.rasterize_glyph(params, raster_bounds)
    }

    fn layout_line(&self, text: &str, font_size: Pixels, runs: &[FontRun]) -> LineLayout {
        let families = {
            let known = self.families.lock();
            runs.iter()
                .filter_map(|run| known.get(run.font_id.0).cloned())
                .collect()
        };
        self.lines.lock().push((text.to_string(), families));
        self.noop.layout_line(text, font_size, runs)
    }

    fn recommended_rendering_mode(&self, font_id: FontId, font_size: Pixels) -> TextRenderingMode {
        self.noop.recommended_rendering_mode(font_id, font_size)
    }
}

/// Runs `test` against a fresh `TestAppContext` whose text system records
/// families, tearing it down the way `#[gpui_kit::test]` does. That macro
/// always builds on the plain `NoopTextSystem`, so these tests are plain
/// `#[test]`s that call this instead.
pub(crate) fn with_recorded_text(test: impl FnOnce(&mut TestAppContext, &RecordingTextSystem)) {
    let recorder = Arc::new(RecordingTextSystem::default());
    let mut cx =
        TestAppContext::build_with_text_system(TestDispatcher::new(0), None, recorder.clone());
    test(&mut cx, &recorder);
    cx.run_until_parked();
    cx.update(|cx| {
        cx.background_executor().forbid_parking();
        cx.quit();
    });
    cx.run_until_parked();
}
