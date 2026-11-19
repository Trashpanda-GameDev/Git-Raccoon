//! Custom themes: a name, the built-in look they started from, and their
//! own colours for its dark and light versions. They're kept in the
//! settings, and can be shared as small JSON files whose colours are
//! plain "#rrggbb".

use eframe::egui::Color32;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::settings::ThemeChoice;
use crate::theme::{self, Palette};

/// A colour, written "#rrggbb".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hex(pub Color32);

impl Hex {
    /// Reads "#rrggbb" (or "rrggbb", or "#rgb").
    pub fn parse(text: &str) -> Option<Hex> {
        let digits = text.trim().trim_start_matches('#');
        let full: String = match digits.len() {
            3 => digits.chars().flat_map(|c| [c, c]).collect(),
            6 => digits.to_string(),
            _ => return None,
        };
        let byte = |i: usize| u8::from_str_radix(full.get(i..i + 2)?, 16).ok();
        Some(Hex(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?)))
    }

    pub fn text(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0.r(), self.0.g(), self.0.b())
    }
}

impl Serialize for Hex {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.text())
    }
}

impl<'de> Deserialize<'de> for Hex {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Hex::parse(&text).ok_or_else(|| {
            serde::de::Error::custom(format!("{text:?} isn't a colour like #1a2b3c"))
        })
    }
}

/// The colours a custom theme sets, for one version (dark or light).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Colours {
    pub background: Hex,
    pub panels: Hex,
    pub hover: Hex,
    pub selected: Hex,
    pub borders: Hex,
    pub text: Hex,
    pub secondary_text: Hex,
    pub accent: Hex,
    pub on_accent: Hex,
    pub added: Hex,
    pub deleted: Hex,
    pub modified: Hex,
    pub renamed: Hex,
    pub danger: Hex,
    /// The graph's lines, in order.
    pub lanes: Vec<Hex>,
}

/// One of the colours, for editing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Background,
    Panels,
    Hover,
    Selected,
    Borders,
    Text,
    SecondaryText,
    Accent,
    OnAccent,
    Added,
    Deleted,
    Modified,
    Renamed,
    Danger,
}

impl Field {
    /// The groups they're shown in, with their names.
    pub const GROUPS: [(&'static str, &'static [Field]); 3] = [
        (
            "Surfaces",
            &[
                Field::Background,
                Field::Panels,
                Field::Hover,
                Field::Selected,
                Field::Borders,
            ],
        ),
        (
            "Text and accent",
            &[
                Field::Text,
                Field::SecondaryText,
                Field::Accent,
                Field::OnAccent,
            ],
        ),
        (
            "Changes",
            &[
                Field::Added,
                Field::Deleted,
                Field::Modified,
                Field::Renamed,
                Field::Danger,
            ],
        ),
    ];

    pub fn name(self) -> &'static str {
        match self {
            Field::Background => "Background",
            Field::Panels => "Panels",
            Field::Hover => "Hover",
            Field::Selected => "Selected",
            Field::Borders => "Borders",
            Field::Text => "Text",
            Field::SecondaryText => "Secondary text",
            Field::Accent => "Accent",
            Field::OnAccent => "Text on accent",
            Field::Added => "Added",
            Field::Deleted => "Deleted",
            Field::Modified => "Modified",
            Field::Renamed => "Renamed",
            Field::Danger => "Danger",
        }
    }
}

impl Colours {
    pub fn get(&self, field: Field) -> Hex {
        *self.slot(field)
    }

    pub fn get_mut(&mut self, field: Field) -> &mut Hex {
        match field {
            Field::Background => &mut self.background,
            Field::Panels => &mut self.panels,
            Field::Hover => &mut self.hover,
            Field::Selected => &mut self.selected,
            Field::Borders => &mut self.borders,
            Field::Text => &mut self.text,
            Field::SecondaryText => &mut self.secondary_text,
            Field::Accent => &mut self.accent,
            Field::OnAccent => &mut self.on_accent,
            Field::Added => &mut self.added,
            Field::Deleted => &mut self.deleted,
            Field::Modified => &mut self.modified,
            Field::Renamed => &mut self.renamed,
            Field::Danger => &mut self.danger,
        }
    }

    fn slot(&self, field: Field) -> &Hex {
        match field {
            Field::Background => &self.background,
            Field::Panels => &self.panels,
            Field::Hover => &self.hover,
            Field::Selected => &self.selected,
            Field::Borders => &self.borders,
            Field::Text => &self.text,
            Field::SecondaryText => &self.secondary_text,
            Field::Accent => &self.accent,
            Field::OnAccent => &self.on_accent,
            Field::Added => &self.added,
            Field::Deleted => &self.deleted,
            Field::Modified => &self.modified,
            Field::Renamed => &self.renamed,
            Field::Danger => &self.danger,
        }
    }

    /// Where text would be hard to read: what, how well it contrasts, and
    /// the colour that would fix it (the text's, against its background).
    /// (Text on the accent isn't among them: buttons darken or lighten the
    /// accent themselves until their text reads.)
    pub fn problems(&self) -> Vec<Problem> {
        let pairs = [
            (Field::Text, Field::Background, "Text on the background"),
            (Field::Text, Field::Panels, "Text on panels"),
            (
                Field::SecondaryText,
                Field::Background,
                "Secondary text on the background",
            ),
            (
                Field::SecondaryText,
                Field::Panels,
                "Secondary text on panels",
            ),
        ];
        pairs
            .into_iter()
            .filter_map(|(text, on, what)| {
                let ratio = theme::contrast(self.get(text).0, self.get(on).0);
                (ratio < READABLE).then_some(Problem {
                    what,
                    ratio,
                    text,
                    on,
                })
            })
            .collect()
    }

    /// Fixes a problem: the text's colour, lighter or darker until it
    /// reads against its background.
    pub fn fix(&mut self, problem: &Problem) {
        let on = self.get(problem.on).0;
        let colour = self.get(problem.text).0;
        *self.get_mut(problem.text) = Hex(readable(colour, on));
    }
}

/// The contrast text needs (WCAG AA, for normal text).
pub const READABLE: f32 = 4.5;

/// Text that's hard to read on its background.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub what: &'static str,
    pub ratio: f32,
    pub text: Field,
    pub on: Field,
}

/// `colour`, moved towards white or black (whichever reads better on
/// `on`) just far enough to read on it.
pub fn readable(colour: Color32, on: Color32) -> Color32 {
    let towards = if theme::contrast(Color32::WHITE, on) >= theme::contrast(Color32::BLACK, on) {
        Color32::WHITE
    } else {
        Color32::BLACK
    };
    for step in 1..=50_u8 {
        let mixed = colour.lerp_to_gamma(towards, f32::from(step) / 50.0);
        if theme::contrast(mixed, on) >= READABLE {
            return mixed;
        }
    }
    towards
}

/// A custom theme.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomTheme {
    pub name: String,
    /// The look it started from (its code colours come from there).
    pub base: ThemeChoice,
    pub dark: Colours,
    pub light: Colours,
}

impl CustomTheme {
    /// A new theme with a look's colours to start from.
    pub fn starting_from(name: &str, base: ThemeChoice) -> CustomTheme {
        let family = base.family();
        CustomTheme {
            name: name.to_string(),
            base,
            dark: family.look(false).palette().colours(),
            light: family.look(true).palette().colours(),
        }
    }

    pub fn colours(&self, light: bool) -> &Colours {
        if light { &self.light } else { &self.dark }
    }

    pub fn colours_mut(&mut self, light: bool) -> &mut Colours {
        if light {
            &mut self.light
        } else {
            &mut self.dark
        }
    }

    /// Its colours for the app, in the dark or light version.
    pub fn palette(&self, light: bool) -> Palette {
        self.base
            .family()
            .look(light)
            .palette()
            .with(self.colours(light))
    }

    /// As a file to share.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// From a shared file. Missing graph lines come from its base look.
    pub fn from_json(text: &str) -> Result<CustomTheme, String> {
        let mut theme: CustomTheme = serde_json::from_str(text)
            .map_err(|e| format!("it isn't a Git Raccoon theme ({e})"))?;
        theme.name = theme.name.trim().to_string();
        if theme.name.is_empty() {
            return Err("the theme has no name".into());
        }
        let family = theme.base.family();
        for (light, colours) in [(false, &mut theme.dark), (true, &mut theme.light)] {
            let base = family.look(light).palette().colours().lanes;
            colours.lanes.truncate(base.len());
            let have = colours.lanes.len();
            colours.lanes.extend(base.into_iter().skip(have));
        }
        Ok(theme)
    }
}

/// `wanted`, or "wanted 2", "wanted 3"… if a theme has that name already.
pub fn unique_name(wanted: &str, themes: &[CustomTheme]) -> String {
    let wanted = wanted.trim();
    let taken = |name: &str| themes.iter().any(|t| t.name == name);
    if !taken(wanted) {
        return wanted.to_string();
    }
    (2..)
        .map(|n| format!("{wanted} {n}"))
        .find(|name| !taken(name))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_colours() {
        assert_eq!(
            Hex::parse("#1a2B3c"),
            Some(Hex(Color32::from_rgb(0x1a, 0x2b, 0x3c)))
        );
        assert_eq!(Hex::parse("fff"), Some(Hex(Color32::WHITE)));
        assert_eq!(Hex::parse("#12345"), None);
        assert_eq!(Hex::parse("#gggggg"), None);
        assert_eq!(Hex(Color32::from_rgb(1, 2, 255)).text(), "#0102ff");
    }

    #[test]
    fn a_new_theme_looks_like_where_it_started() {
        let t = CustomTheme::starting_from("Mine", ThemeChoice::Forest);
        for light in [false, true] {
            let built = t.palette(light);
            let look = *ThemeChoice::Forest.family().look(light).palette();
            assert_eq!(built.colours(), look.colours());
            assert_eq!(built.is_light(), light);
        }
    }

    #[test]
    fn its_colours_reach_the_app() {
        let mut t = CustomTheme::starting_from("Mine", ThemeChoice::Raccoon);
        let blue = Hex(Color32::from_rgb(0x6f, 0xa8, 0xff));
        t.dark.accent = blue;
        t.dark.lanes[0] = blue;
        let p = t.palette(false);
        assert_eq!(p.colours().accent, blue);
        assert_eq!(p.colours().lanes[0], blue);
        assert_eq!(
            t.palette(true).colours().accent,
            t.light.accent,
            "the light version is its own"
        );
    }

    #[test]
    fn shares_as_a_file() {
        let mut t = CustomTheme::starting_from("Midnight Raccoon", ThemeChoice::Raccoon);
        t.dark.text = Hex(Color32::from_rgb(0xdd, 0xe5, 0xf3));
        let json = t.to_json();
        assert!(json.contains("\"text\": \"#dde5f3\""), "{json}");
        assert_eq!(CustomTheme::from_json(&json).unwrap(), t);
        // Fewer graph lines than ten: the rest come from its base look.
        let mut short: serde_json::Value = serde_json::from_str(&json).unwrap();
        short["dark"]["lanes"] = serde_json::json!(["#ff0000"]);
        let read = CustomTheme::from_json(&short.to_string()).unwrap();
        assert_eq!(read.dark.lanes.len(), 10);
        assert_eq!(read.dark.lanes[0], Hex(Color32::RED));
        assert!(CustomTheme::from_json("{\"name\": 3}").is_err());
        assert!(
            CustomTheme::from_json("not json")
                .unwrap_err()
                .contains("isn't a Git Raccoon theme")
        );
    }

    #[test]
    fn finds_hard_to_read_text_and_fixes_it() {
        let mut c = CustomTheme::starting_from("Mine", ThemeChoice::Raccoon).dark;
        assert!(
            c.problems().is_empty(),
            "the built-in look reads well: {:?}",
            c.problems()
        );
        c.secondary_text = Hex(Color32::from_rgb(0x55, 0x52, 0x50));
        let problems = c.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.what == "Secondary text on panels")
        );
        for p in &problems {
            c.fix(p);
        }
        assert!(c.problems().is_empty(), "{:?}", c.problems());
    }

    #[test]
    fn names_stay_different() {
        let themes = vec![
            CustomTheme::starting_from("Mine", ThemeChoice::Raccoon),
            CustomTheme::starting_from("Mine 2", ThemeChoice::Raccoon),
        ];
        assert_eq!(unique_name("Mine", &themes), "Mine 3");
        assert_eq!(unique_name(" Other ", &themes), "Other");
    }
}
