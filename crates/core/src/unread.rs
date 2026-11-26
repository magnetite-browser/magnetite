//! Unread counts in page titles. Chat and mail sites put the number of
//! unread messages into their page's title (`(3) WhatsApp`,
//! `Inbox (12) - Gmail`); the sidebar shows it as a bubble on the tab, and
//! the title without it.

use std::borrow::Cow;

use crate::{SidebarSnapshot, TabItem};

/// Page title `title` split into its unread count and the title without
/// it. A count of 0 is taken out too but gives no count; a title without a
/// count comes back as it is (borrowed).
///
/// Two places count: a number in parentheses at the start (`(3) WhatsApp`,
/// `(12+) Discord`) and one right after the title's first word
/// (`Inbox (12) - Gmail`), each followed by a space or the end of the
/// title (`invoice (1).pdf` is a file name). A plain number of four digits
/// or more reads as a year or a name (`Alien (1979) - IMDb`,
/// `(2024) Annual report`), so such a count needs thousands separators
/// (`Inbox (1,234)`).
pub fn split_unread(title: &str) -> (Option<u32>, Cow<'_, str>) {
    let trimmed = title.trim_start();
    if let Some((count, after)) = trimmed.strip_prefix('(').and_then(count_in_parens) {
        return (count, Cow::Owned(after.trim_start().to_owned()));
    }
    if let Some((word, rest)) = trimmed.split_once(' ')
        && let Some((count, after)) = rest.strip_prefix('(').and_then(count_in_parens)
    {
        return (count, Cow::Owned(format!("{word}{after}")));
    }
    (None, Cow::Borrowed(title))
}

/// The count at the start of `rest` (the text after an opening parenthesis)
/// up to its closing one, `None` for 0, and the text after the closing one.
/// Digits come as one to three, or as groups of three after a first group of
/// one to three, joined by `,` or `.`; a trailing `+` (`99+`) is allowed.
fn count_in_parens(rest: &str) -> Option<(Option<u32>, &str)> {
    let (inner, after) = rest.split_once(')')?;
    if !(after.is_empty() || after.starts_with(char::is_whitespace)) {
        return None;
    }
    let number = inner.strip_suffix('+').unwrap_or(inner);
    let digits = |group: &str| group.bytes().all(|b| b.is_ascii_digit());
    let mut groups = number.split([',', '.']);
    let first_ok = groups
        .next()
        .is_some_and(|g| (1..=3).contains(&g.len()) && digits(g));
    if !first_ok || !groups.all(|g| g.len() == 3 && digits(g)) {
        return None;
    }
    let value: String = number.chars().filter(char::is_ascii_digit).collect();
    // Saturates: "(99,999,999,999)" is still a lot, and shows as "99+".
    let count = value
        .parse::<u64>()
        .map_or(u32::MAX, |n| u32::try_from(n).unwrap_or(u32::MAX));
    Some(((count > 0).then_some(count), after))
}

/// Take the unread count out of tab `t`'s title, unless the tab was renamed.
pub fn strip_count(t: &mut TabItem) {
    if !t.custom_title
        && let (_, Cow::Owned(title)) = split_unread(&t.title)
    {
        t.title = title;
    }
}

/// Take the unread counts out of every tab and bookmark title in `snap`
/// (`strip_count`): titles saved, synced or imported by a version that kept
/// them still carry one.
pub fn strip_counts(snap: &mut SidebarSnapshot) {
    snap.for_each_tab_mut(&mut strip_count);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FavoriteSet;

    fn found(count: u32, rest: &str) -> (Option<u32>, Cow<'_, str>) {
        (Some(count), Cow::Borrowed(rest))
    }

    #[test]
    fn a_count_at_the_start_leaves_the_rest_of_the_title() {
        assert_eq!(split_unread("(1) WhatsApp"), found(1, "WhatsApp"));
        assert_eq!(
            split_unread("(12+) Discord | #general"),
            found(12, "Discord | #general")
        );
        assert_eq!(
            split_unread("(1,234) Feed | LinkedIn"),
            found(1234, "Feed | LinkedIn")
        );
        assert_eq!(split_unread("(3)"), found(3, ""));
    }

    #[test]
    fn a_count_after_the_first_word_is_taken_out_with_its_space() {
        assert_eq!(
            split_unread("Inbox (3) - me@example.org - Gmail"),
            found(3, "Inbox - me@example.org - Gmail")
        );
        assert_eq!(
            split_unread("Posteingang (1.234) - Gmail"),
            found(1234, "Posteingang - Gmail")
        );
        assert_eq!(split_unread("Inbox (1,234)"), found(1234, "Inbox"));
    }

    #[test]
    fn a_count_of_zero_leaves_the_title_but_gives_no_count() {
        assert_eq!(
            split_unread("Inbox (0) - Mail"),
            (None, Cow::Borrowed("Inbox - Mail"))
        );
        assert_eq!(
            split_unread("(0) Nothing new"),
            (None, Cow::Borrowed("Nothing new"))
        );
    }

    #[test]
    fn titles_without_a_count_stay_as_they_are() {
        for title in [
            "WhatsApp",
            "",
            "Alien (1979) - IMDb",
            "(2024) Annual report",
            "Python (programming language) - Wikipedia",
            "Release notes (v2)",
            "The best (3) later",
            "(1,23) Odd groups",
            "() Empty",
            "(3 Unclosed",
            "invoice (1).pdf",
            "Screenshot (3).png (1920×1080)",
        ] {
            assert!(
                matches!(split_unread(title), (None, Cow::Borrowed(t)) if t == title),
                "{title}"
            );
        }
    }

    #[test]
    fn a_huge_count_saturates() {
        assert_eq!(
            split_unread("(99,999,999,999) Spam"),
            found(u32::MAX, "Spam")
        );
    }

    #[test]
    fn saved_titles_lose_their_count_unless_renamed() {
        let mut snap = SidebarSnapshot::new();
        let renamed = TabItem {
            custom_title: true,
            ..TabItem::new("r", "https://example.org/r", "(2) My chat")
        };
        snap.favorites.push(FavoriteSet {
            profile_id: "p".into(),
            items: vec![
                TabItem::new("a", "https://example.org/a", "(7) Chat"),
                renamed,
            ],
        });
        strip_counts(&mut snap);
        let titles: Vec<&str> = snap.all_tabs().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["Chat", "(2) My chat"]);
    }
}
