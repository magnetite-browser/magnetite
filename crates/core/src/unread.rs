//! Unread counts in page titles. Chat and mail sites put the number of
//! unread messages into their page's title (`(3) WhatsApp`,
//! `Inbox (12) - Gmail`); the sidebar shows it as a bubble on the tab, and
//! the title without it.

/// The unread count in page title `title` and the title without it; `None`
/// when the title carries no count (or a count of 0).
///
/// Two places count: a number in parentheses at the start (`(3) WhatsApp`,
/// `(12+) Discord`) and one right after the title's first word
/// (`Inbox (12) - Gmail`). After a word, a plain number of four digits or
/// more reads as a year (`Alien (1979) - IMDb`), so there it counts only
/// with thousands separators (`Inbox (1,234)`).
pub fn unread_count(title: &str) -> Option<(u32, String)> {
    let title = title.trim_start();
    if let Some((count, after)) = title
        .strip_prefix('(')
        .and_then(|rest| count_before_paren(rest, true))
    {
        return Some((count, after.trim_start().to_owned()));
    }
    let (word, rest) = title.split_once(' ')?;
    let (count, after) = count_before_paren(rest.strip_prefix('(')?, false)?;
    Some((count, format!("{word}{after}")))
}

/// The count at the start of `rest` (the text after an opening parenthesis)
/// up to its closing one, and the text after that. Digits may come in groups
/// of three after a first group of one to three, joined by `,` or `.`; a
/// trailing `+` ("99+") is allowed. Plain digits count when `any_length` is
/// set or there are at most three of them.
fn count_before_paren(rest: &str, any_length: bool) -> Option<(u32, &str)> {
    let (inner, after) = rest.split_once(')')?;
    let number = inner.strip_suffix('+').unwrap_or(inner);
    let groups: Vec<&str> = number.split([',', '.']).collect();
    let digits_only = |g: &str| !g.is_empty() && g.bytes().all(|b| b.is_ascii_digit());
    let well_formed = match groups.as_slice() {
        [plain] => digits_only(plain) && (any_length || plain.len() <= 3),
        [first, rest @ ..] => {
            digits_only(first)
                && first.len() <= 3
                && rest.iter().all(|g| digits_only(g) && g.len() == 3)
        }
        [] => false,
    };
    if !well_formed {
        return None;
    }
    // Saturates: "(99999999999)" is still a lot, and shows as "99+".
    let count = groups
        .concat()
        .parse::<u64>()
        .map_or(u32::MAX, |n| u32::try_from(n).unwrap_or(u32::MAX));
    (count > 0).then_some((count, after))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(count: u32, rest: &str) -> (u32, String) {
        (count, rest.to_owned())
    }

    #[test]
    fn a_count_at_the_start_leaves_the_rest_of_the_title() {
        assert_eq!(unread_count("(1) WhatsApp"), Some(found(1, "WhatsApp")));
        assert_eq!(
            unread_count("(12+) Discord | #general"),
            Some(found(12, "Discord | #general"))
        );
        assert_eq!(
            unread_count("(1234) Feed | LinkedIn"),
            Some(found(1234, "Feed | LinkedIn"))
        );
        assert_eq!(unread_count("(3)"), Some(found(3, "")));
    }

    #[test]
    fn a_count_after_the_first_word_is_taken_out_with_its_space() {
        assert_eq!(
            unread_count("Inbox (3) - me@example.org - Gmail"),
            Some(found(3, "Inbox - me@example.org - Gmail"))
        );
        assert_eq!(
            unread_count("Posteingang (1.234) - Gmail"),
            Some(found(1234, "Posteingang - Gmail"))
        );
        assert_eq!(unread_count("Inbox (1,234)"), Some(found(1234, "Inbox")));
    }

    #[test]
    fn titles_without_a_count_stay_as_they_are() {
        for title in [
            "WhatsApp",
            "",
            "Alien (1979) - IMDb",
            "Python (programming language) - Wikipedia",
            "Release notes (v2)",
            "The best (3) later",
            "(0) Nothing new",
            "(1,23) Odd groups",
            "() Empty",
            "(3 Unclosed",
        ] {
            assert_eq!(unread_count(title), None, "{title}");
        }
    }

    #[test]
    fn a_huge_count_saturates() {
        assert_eq!(
            unread_count("(99999999999) Spam"),
            Some(found(u32::MAX, "Spam"))
        );
    }
}
