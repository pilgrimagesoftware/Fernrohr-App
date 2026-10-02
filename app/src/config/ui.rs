use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub theme: Theme,
    pub text_size: TextSize,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            text_size: TextSize::DEFAULT,
        }
    }
}

/// The text-size preference: a percentage of the default size, always one of
/// [`TextSize::STEPS`]. `ui::text_size` turns it into the app's text scale.
///
/// Stored as the bare percentage (`text_size = 110`). A hand-edited value
/// between steps or out of range loads as the nearest step instead of
/// failing the whole file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "u16", into = "u16")]
pub struct TextSize(u16);

impl TextSize {
    /// Every size, in percent: [`crate::consts::TEXT_SIZE_STEPS`].
    pub const STEPS: [u16; 8] = crate::consts::TEXT_SIZE_STEPS;
    pub const DEFAULT: Self = Self(100);
    pub const MIN: Self = Self(Self::STEPS[0]);
    pub const MAX: Self = Self(Self::STEPS[Self::STEPS.len() - 1]);

    pub fn percent(self) -> u16 {
        self.0
    }

    /// The multiplier for sizes and spacing: 1.0 at the default.
    pub fn factor(self) -> f32 {
        f32::from(self.0) / 100.
    }

    /// One step larger, or unchanged at [`TextSize::MAX`].
    pub fn increase(self) -> Self {
        Self::STEPS
            .iter()
            .find(|&&step| step > self.0)
            .map_or(self, |&step| Self(step))
    }

    /// One step smaller, or unchanged at [`TextSize::MIN`].
    pub fn decrease(self) -> Self {
        Self::STEPS
            .iter()
            .rev()
            .find(|&&step| step < self.0)
            .map_or(self, |&step| Self(step))
    }
}

impl Default for TextSize {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Snaps to the nearest step; a tie goes to the smaller one.
impl From<u16> for TextSize {
    fn from(percent: u16) -> Self {
        let nearest = Self::STEPS
            .into_iter()
            .min_by_key(|&step| step.abs_diff(percent))
            .expect("STEPS is not empty");
        Self(nearest)
    }
}

impl From<TextSize> for u16 {
    fn from(size: TextSize) -> Self {
        size.0
    }
}

/// The user's theme preference - `theme::init`/`theme::watch_window` resolve
/// `System` to whichever `ThemeMode` the OS reports, once at startup and
/// again on every live appearance change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
    #[default]
    System,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let config = UiConfig {
            theme: Theme::Dark,
            text_size: TextSize::from(120),
        };
        let text = toml::to_string(&config).unwrap();
        let parsed: UiConfig = toml::from_str(&text).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn tolerates_unknown_fields() {
        let parsed: UiConfig = toml::from_str("theme = \"light\"\nfuture_field = 42\n").unwrap();
        assert_eq!(parsed.theme, Theme::Light);
    }

    #[test]
    fn a_file_without_a_text_size_loads_the_default() {
        let parsed: UiConfig = toml::from_str("theme = \"dark\"\n").unwrap();
        assert_eq!(parsed.text_size, TextSize::DEFAULT);
    }

    #[test]
    fn text_size_is_stored_as_a_bare_percentage() {
        let config = UiConfig {
            theme: Theme::System,
            text_size: TextSize::from(130),
        };
        let text = toml::to_string(&config).unwrap();
        assert!(text.contains("text_size = 130"), "{text}");
    }

    #[test]
    fn an_off_step_or_out_of_range_size_loads_as_the_nearest_step() {
        for (stored, loaded) in [(115, 110), (104, 100), (87, 85), (0, 85), (400, 150)] {
            let parsed: UiConfig = toml::from_str(&format!("text_size = {stored}\n")).unwrap();
            assert_eq!(parsed.text_size.percent(), loaded, "stored {stored}");
        }
    }

    #[test]
    fn increase_and_decrease_walk_every_step_and_stop_at_the_bounds() {
        let mut size = TextSize::MIN;
        let mut seen = vec![size.percent()];
        while size != TextSize::MAX {
            size = size.increase();
            seen.push(size.percent());
        }
        assert_eq!(seen, TextSize::STEPS);
        assert_eq!(TextSize::MAX.increase(), TextSize::MAX);
        assert_eq!(TextSize::MIN.decrease(), TextSize::MIN);
        assert_eq!(TextSize::DEFAULT.decrease().percent(), 90);
        assert_eq!(TextSize::DEFAULT.increase().percent(), 110);
    }

    #[test]
    fn the_default_is_100_percent_and_a_factor_of_one() {
        assert_eq!(TextSize::default().percent(), 100);
        assert_eq!(TextSize::DEFAULT.factor(), 1.0);
        assert_eq!(TextSize::MAX.factor(), 1.5);
    }
}
