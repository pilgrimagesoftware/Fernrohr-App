//! The icon catalog the app registers (`with_assets`): gpui-kit's full Lucide
//! catalog, not its default bundle. The default bundle holds only the 104 icons
//! gpui-kit's own components draw, so an icon the app names itself from outside
//! it - the theme switcher's `Monitor`, the status bar's `Plug` and `KeyRound`,
//! the Resource panel's `ArrowLeftRight`, the picker's `Server` - drew as nothing:
//! the element sized and clickable, the glyph absent. The full catalog is about
//! 1.1MB of SVG, embedded.

/// The asset source `main` registers.
pub(crate) use gpui_kit::assets::AllAssets as AppAssets;

#[cfg(test)]
mod tests {
    use super::AppAssets;
    use gpui_kit::AssetSource as _;
    use gpui_kit::assets::IconName;

    /// Every icon the catalog names loads from the source the app registers - the
    /// default bundle fails this for `Monitor` and the four others it lacks.
    #[test]
    fn every_named_icon_loads_from_the_apps_asset_source() {
        let missing: Vec<_> = IconName::ALL
            .iter()
            .filter(|icon| !matches!(AppAssets.load(&icon.path()), Ok(Some(_))))
            .collect();
        assert!(missing.is_empty(), "icons with no SVG: {missing:?}");
    }

    /// The ones the app draws that the default bundle left out, by name.
    #[test]
    fn the_icons_the_default_bundle_lacked_load() {
        for icon in [
            IconName::Monitor,
            IconName::Plug,
            IconName::KeyRound,
            IconName::ArrowLeftRight,
            IconName::Server,
        ] {
            assert!(
                matches!(AppAssets.load(&icon.path()), Ok(Some(_))),
                "{icon:?} loads"
            );
        }
    }
}
