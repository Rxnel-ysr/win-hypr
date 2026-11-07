//! Notification text hygiene: strip the tiny HTML subset apps send, then clip by characters.
//! Both steps avoid allocating when there is nothing to do.

use std::borrow::Cow;

/// Remove `<b>`-style tags and decode the five basic entities.
pub fn plain(s: &str) -> Cow<'_, str> {
    if !s.contains(['<', '&']) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        let (head, tail) = rest.split_at(i);
        out.push_str(head);
        let looks_like_tag = tail[1..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '/');
        match (looks_like_tag, tail.find('>')) {
            (true, Some(end)) => rest = &tail[end + 1..],
            _ => {
                out.push('<'); // a literal "a < b"
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    if out.contains('&') {
        out = out
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&");
    }
    Cow::Owned(out)
}

/// At most `max` characters.
pub fn clip(s: &str, max: usize) -> String {
    if s.len() <= max {
        // bytes <= max implies chars <= max
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

pub fn clean(s: &str, max: usize) -> String {
    clip(&plain(s), max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tags_and_entities() {
        assert_eq!(plain("<b>Hi</b> &amp; bye"), "Hi & bye");
    }

    #[test]
    fn keeps_literal_less_than() {
        assert_eq!(plain("a < b"), "a < b");
        assert_eq!(plain("1 <2> 3"), "1 <2> 3");
    }

    #[test]
    fn clips_on_char_boundary() {
        assert_eq!(clip("héllo", 3), "hél");
        assert_eq!(clip("abc", 10), "abc");
    }
}
