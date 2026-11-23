//! Emoji by their short names (":tada:"), as reactions and comments on
//! GitHub and GitLab use them.

use std::borrow::Cow;

/// The emoji for the short names most used in reviews (GitLab's names,
/// which GitHub's reactions are turned into; see `Reaction::github`).
/// Without the variation selector, which the fonts don't draw.
const TABLE: &[(&str, &str)] = &[
    ("+1", "👍"),
    ("-1", "👎"),
    ("100", "💯"),
    ("beers", "🍻"),
    ("bug", "🐛"),
    ("bulb", "💡"),
    ("clap", "👏"),
    ("coffee", "☕"),
    ("confused", "😕"),
    ("cry", "😢"),
    ("eyes", "👀"),
    ("fire", "🔥"),
    ("grinning", "😀"),
    ("heart", "❤"),
    ("heart_eyes", "😍"),
    ("heavy_check_mark", "✔"),
    ("joy", "😂"),
    ("laughing", "😆"),
    ("muscle", "💪"),
    ("ok_hand", "👌"),
    ("party_popper", "🎉"),
    ("point_up", "☝"),
    ("pray", "🙏"),
    ("rage", "😡"),
    ("raised_hands", "🙌"),
    ("rocket", "🚀"),
    ("scream", "😱"),
    ("slight_smile", "🙂"),
    ("smile", "😄"),
    ("smiley", "😃"),
    ("sob", "😭"),
    ("sparkles", "✨"),
    ("star", "⭐"),
    ("sunglasses", "😎"),
    ("sweat_smile", "😅"),
    ("tada", "🎉"),
    ("thinking", "🤔"),
    ("thumbsdown", "👎"),
    ("thumbsup", "👍"),
    ("trophy", "🏆"),
    ("warning", "⚠"),
    ("wave", "👋"),
    ("white_check_mark", "✅"),
    ("wink", "😉"),
    ("x", "❌"),
];

/// The emoji called `name` ("tada"), if it's one we know.
pub fn glyph(name: &str) -> Option<&'static str> {
    TABLE.iter().find(|(n, _)| *n == name).map(|(_, e)| *e)
}

/// How a reaction shows: its emoji, or ":name:" for one we don't know.
pub fn shown(name: &str) -> Cow<'static, str> {
    match glyph(name) {
        Some(e) => Cow::Borrowed(e),
        None => Cow::Owned(format!(":{name}:")),
    }
}

/// `text` with the short names we know (":tada:") as their emoji; others
/// are left as they are.
pub fn replace_short_names(text: &str) -> Cow<'_, str> {
    if !text.contains(':') {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut changed = false;
    while let Some(start) = rest.find(':') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let name_end = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '+' || c == '-'))
            .unwrap_or(after.len());
        let name = &after[..name_end];
        if !name.is_empty()
            && after[name_end..].starts_with(':')
            && let Some(e) = glyph(name)
        {
            out.push_str(e);
            rest = &after[name_end + 1..];
            changed = true;
        } else {
            out.push(':');
            rest = after;
        }
    }
    out.push_str(rest);
    if changed {
        Cow::Owned(out)
    } else {
        Cow::Borrowed(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_become_emoji() {
        assert_eq!(shown("tada"), "🎉");
        assert_eq!(shown("party_parrot"), ":party_parrot:");
        assert_eq!(
            replace_short_names(":+1: looks good :tada:"),
            "👍 looks good 🎉"
        );
        assert_eq!(
            replace_short_names("at 10:30: done :nope: ok"),
            "at 10:30: done :nope: ok"
        );
        assert_eq!(replace_short_names("a::b :x"), "a::b :x");
        assert!(matches!(replace_short_names("plain"), Cow::Borrowed(_)));
    }

    #[test]
    fn the_table_is_sorted_and_has_no_repeats() {
        assert!(TABLE.windows(2).all(|w| w[0].0 < w[1].0));
    }
}
