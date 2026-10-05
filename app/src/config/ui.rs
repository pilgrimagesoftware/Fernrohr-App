use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub theme: Theme,
    pub text_size: TextSize,
    /// The window edge a new window's Resource panel opens on. Moving the panel
    /// in a window doesn't change it; Make Resource Panel's Side the Default does.
    pub resource_side: ResourceSide,
    /// How far back a new pod detail panel's Events tab looks
    /// (`pod-events-time-window` 2.2). Each panel can change its own.
    pub pod_events_window: PodEventsWindow,
    /// How long a chord whose keys so far are also a whole binding waits for
    /// its next key (`pending-chord-indicator`).
    pub shortcut_timeout_secs: ShortcutTimeout,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            text_size: TextSize::DEFAULT,
            resource_side: ResourceSide::default(),
            pod_events_window: PodEventsWindow::default(),
            shortcut_timeout_secs: ShortcutTimeout::DEFAULT,
        }
    }
}

/// The Shortcut timeout preference, in whole seconds from
/// [`ShortcutTimeout::MIN`] to [`ShortcutTimeout::MAX`].
///
/// Stored as the bare number (`shortcut_timeout_secs = 3`). Read through a
/// wide integer and clamped, so a hand-edited value out of range - even one
/// too big for the field, or negative - loads as the nearest allowed value
/// instead of failing the whole file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "i64", into = "i64")]
pub struct ShortcutTimeout(u8);

impl ShortcutTimeout {
    pub const MIN: Self = Self(crate::consts::SHORTCUT_TIMEOUT_MIN_SECS);
    pub const MAX: Self = Self(crate::consts::SHORTCUT_TIMEOUT_MAX_SECS);
    pub const DEFAULT: Self = Self(crate::consts::SHORTCUT_TIMEOUT_DEFAULT_SECS);

    pub fn secs(self) -> u8 {
        self.0
    }

    pub fn duration(self) -> std::time::Duration {
        std::time::Duration::from_secs(u64::from(self.0))
    }

    /// One second longer, or unchanged at [`ShortcutTimeout::MAX`].
    pub fn increase(self) -> Self {
        Self::from(i64::from(self.0) + 1)
    }

    /// One second shorter, or unchanged at [`ShortcutTimeout::MIN`].
    pub fn decrease(self) -> Self {
        Self::from(i64::from(self.0) - 1)
    }
}

impl Default for ShortcutTimeout {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Clamps into [`ShortcutTimeout::MIN`]..=[`ShortcutTimeout::MAX`].
impl From<i64> for ShortcutTimeout {
    fn from(secs: i64) -> Self {
        let clamped = secs.clamp(i64::from(Self::MIN.0), i64::from(Self::MAX.0));
        Self(u8::try_from(clamped).expect("clamped into the u8 range"))
    }
}

impl From<ShortcutTimeout> for i64 {
    fn from(timeout: ShortcutTimeout) -> Self {
        i64::from(timeout.0)
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

/// A window edge the Resource panel docks to. `ui::resource_panel` draws it;
/// this is the stored vocabulary (`resource_side = "right"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceSide {
    #[default]
    Left,
    Right,
}

/// How far back a pod's Events tab looks: events last seen within it are shown,
/// the rest counted as hidden. Stored as `pod_events_window = "1h"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PodEventsWindow {
    #[serde(rename = "15m")]
    Minutes15,
    #[default]
    #[serde(rename = "1h")]
    Hour1,
    #[serde(rename = "6h")]
    Hours6,
    #[serde(rename = "24h")]
    Hours24,
    #[serde(rename = "all")]
    All,
}

impl PodEventsWindow {
    /// Every window, shortest first.
    pub const ALL: [Self; 5] = [
        Self::Minutes15,
        Self::Hour1,
        Self::Hours6,
        Self::Hours24,
        Self::All,
    ];

    /// How far back it reaches; `None` for All.
    pub fn span(self) -> Option<std::time::Duration> {
        let minutes = match self {
            Self::Minutes15 => 15,
            Self::Hour1 => 60,
            Self::Hours6 => 6 * 60,
            Self::Hours24 => 24 * 60,
            Self::All => return None,
        };
        Some(std::time::Duration::from_secs(minutes * 60))
    }

    /// The next shorter window, or this one when it's already the shortest.
    pub fn shorter(self) -> Self {
        let at = Self::ALL
            .iter()
            .position(|window| *window == self)
            .unwrap_or(0);
        Self::ALL[at.saturating_sub(1)]
    }

    /// The next longer window, or this one when it's already All.
    pub fn longer(self) -> Self {
        let at = Self::ALL
            .iter()
            .position(|window| *window == self)
            .unwrap_or(0);
        Self::ALL[(at + 1).min(Self::ALL.len() - 1)]
    }

    /// What the selector and the empty-state wording call it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Minutes15 => "15 minutes",
            Self::Hour1 => "1 hour",
            Self::Hours6 => "6 hours",
            Self::Hours24 => "24 hours",
            Self::All => "All",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let config = UiConfig {
            theme: Theme::Dark,
            text_size: TextSize::from(120),
            resource_side: ResourceSide::Right,
            pod_events_window: PodEventsWindow::Hours6,
            shortcut_timeout_secs: ShortcutTimeout::from(7),
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
    fn the_resource_side_is_stored_by_name_and_defaults_to_left() {
        let parsed: UiConfig = toml::from_str("resource_side = \"right\"\n").unwrap();
        assert_eq!(parsed.resource_side, ResourceSide::Right);
        let parsed: UiConfig = toml::from_str("theme = \"dark\"\n").unwrap();
        assert_eq!(parsed.resource_side, ResourceSide::Left);
    }

    #[test]
    fn the_pod_events_window_is_stored_by_name_and_defaults_to_an_hour() {
        let parsed: UiConfig = toml::from_str("pod_events_window = \"6h\"\n").unwrap();
        assert_eq!(parsed.pod_events_window, PodEventsWindow::Hours6);
        let parsed: UiConfig = toml::from_str("theme = \"dark\"\n").unwrap();
        assert_eq!(parsed.pod_events_window, PodEventsWindow::Hour1);
        let text = toml::to_string(&UiConfig {
            pod_events_window: PodEventsWindow::All,
            ..UiConfig::default()
        })
        .unwrap();
        assert!(text.contains("pod_events_window = \"all\""), "{text}");
    }

    #[test]
    fn text_size_is_stored_as_a_bare_percentage() {
        let config = UiConfig {
            theme: Theme::System,
            text_size: TextSize::from(130),
            ..UiConfig::default()
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

    #[test]
    fn the_shortcut_timeout_defaults_to_three_and_is_a_bare_number() {
        let parsed: UiConfig = toml::from_str("theme = \"dark\"\n").unwrap();
        assert_eq!(parsed.shortcut_timeout_secs, ShortcutTimeout::DEFAULT);
        assert_eq!(parsed.shortcut_timeout_secs.secs(), 3);
        let config = UiConfig {
            shortcut_timeout_secs: ShortcutTimeout::from(6),
            ..UiConfig::default()
        };
        assert!(
            toml::to_string(&config)
                .unwrap()
                .contains("shortcut_timeout_secs = 6\n")
        );
    }

    /// Spec: "Out-of-range value in the file" - clamped, the rest kept.
    #[test]
    fn an_out_of_range_shortcut_timeout_clamps_and_keeps_the_rest() {
        for (written, loads) in [("30", 10), ("300", 10), ("0", 1), ("-4", 1)] {
            let parsed: UiConfig = toml::from_str(&format!(
                "theme = \"dark\"\ntext_size = 120\nshortcut_timeout_secs = {written}\n"
            ))
            .unwrap();
            assert_eq!(parsed.shortcut_timeout_secs.secs(), loads, "{written}");
            assert_eq!(parsed.theme, Theme::Dark, "{written} keeps the theme");
            assert_eq!(parsed.text_size, TextSize::from(120));
        }
    }

    #[test]
    fn the_shortcut_timeout_steps_by_a_second_within_its_range() {
        assert_eq!(ShortcutTimeout::DEFAULT.increase().secs(), 4);
        assert_eq!(ShortcutTimeout::DEFAULT.decrease().secs(), 2);
        assert_eq!(ShortcutTimeout::MAX.increase(), ShortcutTimeout::MAX);
        assert_eq!(ShortcutTimeout::MIN.decrease(), ShortcutTimeout::MIN);
    }
}
