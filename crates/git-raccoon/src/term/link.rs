//! What a word in a terminal's output points at: a commit, a file at a
//! line, or a web address.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    Url(String),
    /// 7 to 40 hex digits (to be checked against the repository).
    Commit(String),
    /// A file, relative to the repository (or absolute), at a line.
    File {
        path: String,
        line: usize,
    },
}

/// What `word` points at, if anything. Quotes, brackets and trailing
/// punctuation around it don't count.
pub fn classify(word: &str) -> Option<Link> {
    let w = word
        .trim_start_matches(['(', '[', '<', '"', '\'', '`', '{'])
        .trim_end_matches([')', ']', '>', '"', '\'', '`', '}', ',', ';', '.', '!', '?']);
    if w.starts_with("https://") || w.starts_with("http://") {
        return Some(Link::Url(w.to_string()));
    }
    if (7..=40).contains(&w.len()) && w.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(Link::Commit(w.to_ascii_lowercase()));
    }
    file_at_line(w)
}

/// `src/a.rs:42`, `src/a.rs:42:17`, `src/a.rs(42)`, `src/a.rs(42,17)`, or a
/// bare `src/a.rs` (line 1).
fn file_at_line(w: &str) -> Option<Link> {
    let (path, rest) = match w.find(['(', ':']) {
        // A Windows drive ("C:\…"): the colon after it is part of the path.
        Some(1)
            if w.as_bytes()
                .get(2)
                .is_some_and(|b| *b == b'\\' || *b == b'/') =>
        {
            match w[2..].find(['(', ':']) {
                Some(i) => (&w[..i + 2], &w[i + 2..]),
                None => (w, ""),
            }
        }
        Some(i) => (&w[..i], &w[i..]),
        None => (w, ""),
    };
    let looks_like_file = path.contains('.')
        && !path.ends_with('.')
        && !path.starts_with('-')
        && path.chars().any(|c| c.is_alphabetic())
        && (path.contains('/')
            || path.contains('\\')
            || path.rsplit_once('.').is_some_and(|(_, e)| e.len() <= 5));
    if !looks_like_file || path.contains("://") {
        return None;
    }
    let digits: String = rest
        .trim_start_matches([':', '('])
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    if !rest.is_empty() && digits.is_empty() {
        return None;
    }
    Some(Link::File {
        path: path.to_string(),
        line: digits.parse().unwrap_or(1).max(1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_commits_files_and_links() {
        assert_eq!(classify("a41f9c2"), Some(Link::Commit("a41f9c2".into())));
        assert_eq!(classify("(A41F9C2)"), Some(Link::Commit("a41f9c2".into())));
        assert_eq!(classify("abc"), None, "too short for a commit");
        assert_eq!(
            classify("src/store.rs:42:17"),
            Some(Link::File {
                path: "src/store.rs".into(),
                line: 42
            })
        );
        assert_eq!(
            classify("src\\store.cs(42,17):"),
            Some(Link::File {
                path: "src\\store.cs".into(),
                line: 42
            })
        );
        assert_eq!(
            classify("C:\\code\\a.rs:7"),
            Some(Link::File {
                path: "C:\\code\\a.rs".into(),
                line: 7
            })
        );
        assert_eq!(
            classify("README.md"),
            Some(Link::File {
                path: "README.md".into(),
                line: 1
            })
        );
        assert_eq!(
            classify("<https://example.com/a?b=1>."),
            Some(Link::Url("https://example.com/a?b=1".into()))
        );
        assert_eq!(classify("hello"), None);
        assert_eq!(classify("1.2.3"), None, "a version, not a file");
        assert_eq!(classify("--force"), None);
    }
}
