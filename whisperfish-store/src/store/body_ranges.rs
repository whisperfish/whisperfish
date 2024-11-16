use crate::store::protos as database_protos;
pub use database_protos::body_range_list::{body_range::AssociatedValue, BodyRange};
use itertools::Itertools;
use libsignal_service::proto::{
    body_range as wire_body_range, body_range::Style as WireStyle, BodyRange as WireBodyRange,
};
use prost::Message;

pub const SPOILER_TAG_UNCLICKED: &str =
    "<span style='background-color: \"white\"; color: \"white\";'>";
pub const SPOILER_TAG_CLICKED: &str = "<span>";
pub const TOUCHING_SPOILERS: &str =
    "</span><span style='background-color: \"white\"; color: \"white\";'>";

// Note the trailing spaces.
pub const LINK_TAG_UNCLICKED: &str = "<a style='background-color: \"white\"; color: \"white\";' ";
pub const LINK_TAG_CLICKED: &str = "<a ";

pub fn deserialize(message_ranges: &[u8]) -> Vec<database_protos::body_range_list::BodyRange> {
    let message_ranges = database_protos::BodyRangeList::decode(message_ranges as &[u8])
        .expect("valid protobuf in database");
    message_ranges.ranges
}

#[tracing::instrument(level = "debug", name = "body_ranges::serialize")]
pub fn serialize(value: &[WireBodyRange]) -> Option<Vec<u8>> {
    if value.is_empty() {
        return None;
    }

    let message_ranges = database_protos::BodyRangeList {
        ranges: value
            .iter()
            .map(|range| {
                tracing::trace!(av = ?range.associated_value, start = range.start, len = range.length, "processing range");
                database_protos::body_range_list::BodyRange {
                    start: range.start.expect("start") as i32,
                    length: range.length.expect("end") as i32,
                    associated_value: range.associated_value.as_ref().and_then(|av| match av {
                        wire_body_range::AssociatedValue::MentionAci(mention_aci) => {
                            Some(database_protos::body_range_list::body_range::AssociatedValue::MentionUuid(
                                mention_aci.clone(),
                            ))
                        }
                        wire_body_range::AssociatedValue::Style(style) => {
                            use database_protos::body_range_list::body_range::Style;
                            let style = match WireStyle::try_from(*style).unwrap() {
                                WireStyle::Bold => Some(Style::Bold),
                                WireStyle::Italic => Some(Style::Italic),
                                WireStyle::Spoiler => Some(Style::Spoiler),
                                WireStyle::Strikethrough => Some(Style::Strikethrough),
                                WireStyle::Monospace => Some(Style::Monospace),
                                WireStyle::None => None,
                            };
                            style.map(Into::<i32>::into).map(database_protos::body_range_list::body_range::AssociatedValue::Style)
                        }
                    }),
                }
            })
            .collect(),
    };

    Some(message_ranges.encode_to_vec())
}

#[tracing::instrument(level = "debug", skip(message_ranges), fields(message_ranges_len = message_ranges.map(Vec::len)), name="body_ranges::to_vec")]
pub fn to_vec(message_ranges: Option<&Vec<u8>>) -> Vec<WireBodyRange> {
    let Some(message_ranges) = message_ranges else {
        return vec![];
    };

    deserialize(message_ranges)
        .iter()
        .flat_map(|range| {
            let associated_value = match range
                .associated_value
                .as_ref()
                .expect("associated_value in db")
            {
                database_protos::body_range_list::body_range::AssociatedValue::MentionUuid(
                    mention_aci,
                ) => wire_body_range::AssociatedValue::MentionAci(mention_aci.clone()),
                database_protos::body_range_list::body_range::AssociatedValue::Style(style) => {
                    use database_protos::body_range_list::body_range::Style;
                    wire_body_range::AssociatedValue::Style(match Style::try_from(*style).unwrap() {
                        Style::Bold => WireStyle::Bold,
                        Style::Italic => WireStyle::Italic,
                        Style::Spoiler => WireStyle::Spoiler,
                        Style::Strikethrough => WireStyle::Strikethrough,
                        Style::Monospace => WireStyle::Monospace,
                    }.into())
                }
                database_protos::body_range_list::body_range::AssociatedValue::Link(link) => {
                    tracing::warn!("Not encoding link {link}");
                    return None;
                }
                database_protos::body_range_list::body_range::AssociatedValue::Button(button) => {
                    tracing::warn!("Not encoding button {button:?}");
                    return None;
                }
            };

            tracing::trace!(av = ?range.associated_value, start = range.start, len = range.length, "processed range");

            Some(WireBodyRange {
                start: Some(range.start as u32),
                length: Some(range.length as u32),
                associated_value: Some(associated_value),
            })
        })
        .collect()
}

fn escape_pre(s: &str) -> std::borrow::Cow<'_, str> {
    if s.contains('<') || s.contains('>') || s.contains('&') {
        std::borrow::Cow::Owned(
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;"),
        )
    } else {
        std::borrow::Cow::Borrowed(s)
    }
}

fn escape(s: &str) -> std::borrow::Cow<'_, str> {
    if s.contains('<') || s.contains('>') || s.contains('&') || s.contains('\n') {
        std::borrow::Cow::Owned(
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('\n', "<br>"),
        )
    } else {
        std::borrow::Cow::Borrowed(s)
    }
}

/// Returns a styled message, with ranges for bold, italic, links, quotes, etc.
#[tracing::instrument(level = "debug", skip(mention_lookup))]
pub fn to_styled<'a, S: AsRef<str> + 'a>(
    message: &'a str,
    ranges: &'a [BodyRange],
    mention_lookup: impl Fn(&'a str) -> S,
) -> std::borrow::Cow<'a, str> {
    #[derive(Debug)]
    struct Segment<'a> {
        contents: &'a str,
        start: usize,
        bold: bool,
        italic: bool,
        spoiler: bool,
        strikethrough: bool,
        monospace: bool,
        mention: Option<&'a str>,
        link: Option<&'a str>,
    }

    impl<'a> Segment<'a> {
        fn end(&self) -> usize {
            self.start + self.contents.encode_utf16().count()
        }

        fn split_at(&self, char_idx: usize) -> [Self; 2] {
            if self.mention.is_some() {
                tracing::warn!("splitting a mention");
            }

            // Map UTF16 index to character boundary, by counting UTF16 code units.
            let fold = self.contents.char_indices().fold_while(
                (0, 0),
                |(_utf8_pos, utf16_pos), (pos, c)| {
                    use itertools::FoldWhile::{Continue, Done};

                    let next = (pos, utf16_pos + c.len_utf16());
                    if utf16_pos >= char_idx {
                        Done(next)
                    } else {
                        Continue(next)
                    }
                },
            );
            if !fold.is_done() {
                tracing::warn!(segment=?self, %char_idx, "Fold went out of bounds. Please file an issue.");
            };

            let (idx, _utf16_pos) = fold.into_inner();
            if _utf16_pos < char_idx {
                tracing::warn!(segment=?self, %char_idx, "_utf16_pos < char_idx: out of bounds.  Please file an issue.");
            }

            if cfg!(debug_assertions) {
                let lhs: Vec<u16> = self.contents.encode_utf16().take(char_idx).collect();
                assert_eq!(idx, String::from_utf16(&lhs).unwrap().len());
            }

            [
                Segment {
                    contents: &self.contents[..idx],
                    start: self.start,
                    bold: self.bold,
                    italic: self.italic,
                    spoiler: self.spoiler,
                    strikethrough: self.strikethrough,
                    monospace: self.monospace,
                    mention: self.mention,
                    link: self.link,
                },
                Segment {
                    contents: &self.contents[idx..],
                    start: self.start + char_idx,
                    bold: self.bold,
                    italic: self.italic,
                    spoiler: self.spoiler,
                    strikethrough: self.strikethrough,
                    monospace: self.monospace,
                    mention: None,
                    link: self.link,
                },
            ]
        }
    }

    let finder = linkify::LinkFinder::new();
    let spans = finder.spans(message);
    let mut segments: Vec<_> = spans
        .map(|span| {
            // XXX map this to character index!
            let start = span.start();
            let start_utf16 = message[..start].encode_utf16().count();
            Segment {
                contents: span.as_str(),
                start: start_utf16,
                bold: false,
                italic: false,
                spoiler: false,
                strikethrough: false,
                monospace: false,
                mention: None,
                link: span.kind().map(|kind| match kind {
                    linkify::LinkKind::Url => span.as_str(),
                    linkify::LinkKind::Email => span.as_str(),
                    _ => {
                        tracing::warn!("Unknown LinkKind: {:?}", kind);
                        span.as_str()
                    }
                }),
            }
        })
        .collect();

    // If there are no segments, ranges or special characters we can just return the message without reallocating.
    if segments.len() == 1 && segments[0].link.is_none() && ranges.is_empty() {
        return escape(message);
    }

    fn annotate<'a>(segment: &'_ mut Segment<'a>, style: Option<&'a AssociatedValue>) {
        let Some(style) = style else { return };
        match style {
            AssociatedValue::Style(0) => segment.bold = true,
            AssociatedValue::Style(1) => segment.italic = true,
            AssociatedValue::Style(2) => segment.spoiler = true,
            AssociatedValue::Style(3) => segment.strikethrough = true,
            AssociatedValue::Style(4) => segment.monospace = true,
            AssociatedValue::MentionUuid(s) => {
                assert_eq!(segment.contents.encode_utf16().count(), 1);
                assert_eq!(segment.contents, "\u{fffc}");
                segment.mention = Some(s);
            }
            _ => {}
        }
    }

    // Every BodyRange splits one or two segments into two, and adds a style to the affected segment.
    for range in ranges {
        let _span = tracing::debug_span!("processing range", ?range, segments=?segments).entered();
        // XXX Just skip the range if necessary, that's healthier than panicking.
        let end = (range.start + range.length) as usize;

        if end > message.encode_utf16().count() {
            tracing::warn!(range=?range, "range end out of bounds");
            return std::borrow::Cow::Borrowed(message);
        }

        let left = segments
            .binary_search_by(|segment| {
                if segment.end() <= range.start as usize {
                    std::cmp::Ordering::Less
                } else if segment.start > range.start as usize {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .unwrap_or_else(|e| {
                panic!(
                    "range ({} -> {}) start in segment Err({})",
                    range.start, range.length, e
                )
            });

        let left_split_at = range.start as usize - segments[left].start;
        if left_split_at != 0 {
            segments.splice(left..=left, segments[left].split_at(left_split_at));
        }

        let right = segments
            .binary_search_by(|segment| {
                if segment.end() < end {
                    std::cmp::Ordering::Less
                } else if segment.start > end {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .unwrap_or_else(|e| {
                panic!(
                    "range ({} -> {}) end in segment Err({})",
                    range.start, range.length, e
                )
            });

        let right_split_at = end - segments[right].start;
        if right_split_at != segments[right].contents.encode_utf16().count() {
            segments.splice(right..=right, segments[right].split_at(right_split_at));
        }

        let left = if left_split_at != 0 { left + 1 } else { left };
        for segment in &mut segments[left..=right] {
            annotate(segment, range.associated_value.as_ref());
        }
    }

    segments
        .into_iter()
        .map(|segment| {
            let mut result = String::new();
            let tags = [
                (segment.bold, "b"),
                (segment.italic, "i"),
                (segment.strikethrough, "s"),
            ];

            for (add_tag, tag) in &tags {
                if *add_tag {
                    result.push('<');
                    result.push_str(tag);
                    result.push('>');
                }
            }

            // XXX Optimise: only insert spoiler start if previous segment was not a spoiler
            if segment.spoiler {
                result.push_str(SPOILER_TAG_UNCLICKED);
            }

            if let Some(mention) = segment.mention {
                if segment.spoiler {
                    result.push_str(LINK_TAG_UNCLICKED);
                } else {
                    result.push_str(LINK_TAG_CLICKED);
                }
                result.push_str("href=\"mention://");
                result.push_str(mention);
                result.push_str("\">@");
                result.push_str(mention_lookup(mention).as_ref());
                result.push_str("</a>");
            } else if let Some(link) = segment.link {
                if segment.spoiler {
                    result.push_str(LINK_TAG_UNCLICKED);
                } else {
                    result.push_str(LINK_TAG_CLICKED);
                }
                result.push_str("href=\"");
                result.push_str(link);
                result.push_str("\">");
                result.push_str(&escape(segment.contents));
                result.push_str("</a>");
            } else if segment.monospace {
                result.push_str("<pre>");
                result.push_str(&escape_pre(segment.contents));
                result.push_str("</pre>");
            } else {
                result.push_str(&escape(segment.contents));
            }

            // XXX Optimise: only insert spoiler end if next segment is not a spoiler
            if segment.spoiler {
                result.push_str("</span>");
            }

            for (add_tag, tag) in tags.iter().rev() {
                if *add_tag {
                    result.push_str("</");
                    result.push_str(tag);
                    result.push('>');
                }
            }

            result
        })
        .join("")
        .replace(TOUCHING_SPOILERS, "")
        .into()
}

#[cfg(test)]
mod tests {
    use database_protos::body_range_list::body_range::Style;
    use rstest::rstest;
    use std::borrow::Cow;

    use super::*;

    #[test]
    fn roundtrip_recoding() {
        let input_ranges = vec![WireBodyRange {
            start: Some(0),
            length: Some(1),
            associated_value: Some(wire_body_range::AssociatedValue::MentionAci(
                "9d4428ab-0000-0000-0000-000000000000".to_string(),
            )),
        }];

        let db_ranges = super::serialize(&input_ranges).expect("serialize");
        let output_ranges = super::to_vec(Some(&db_ranges));
        assert_eq!(input_ranges, output_ranges);
    }

    fn no_mentions(u: &str) -> &str {
        panic!("requested mention {u}");
    }

    #[rstest]
    #[case("bXXXbbb", "b<b>XXX</b>bbb")]
    #[case("bXXX", "b<b>XXX</b>")]
    fn styled_simple(#[case] text: &str, #[case] expected: &str) {
        let ranges = super::deserialize(&[10, 6, 8, 1, 16, 3, 32, 0]);
        println!("{ranges:?}");
        let styled = to_styled(text, &ranges, no_mentions);
        assert_eq!(styled, *expected);
    }

    #[test]
    fn nothing_to_show() {
        let ranges = [];
        let msg = "Nothing at all";
        assert_eq!(to_styled(msg, &ranges, no_mentions), Cow::Borrowed(msg))
    }

    #[test]
    fn just_a_link() {
        let ranges = [];
        let msg = "https://www.example.com";
        let styled = to_styled(msg, &ranges, no_mentions);
        assert!(matches!(styled, Cow::Owned(_)));
        assert_eq!(
            styled.as_ref(),
            "<a href=\"https://www.example.com\">https://www.example.com</a>"
        );
    }

    #[test]
    fn mention() {
        let text = "\u{fffc}Sorry for the random mention.";
        let ranges = super::deserialize(&[
            10, 40, 16, 1, 26, 36, 57, 100, 52, 52, 50, 56, 97, 98, 45, 48, 48, 48, 48, 45, 48, 48,
            48, 48, 45, 48, 48, 48, 48, 45, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48,
        ]);
        println!("{ranges:?}");
        let styled = to_styled(text, &ranges, |_u| {
            assert_eq!(_u, "9d4428ab-0000-0000-0000-000000000000");
            "foo"
        });

        assert_eq!(styled, "<a href=\"mention://9d4428ab-0000-0000-0000-000000000000\">@foo</a>Sorry for the random mention.");
    }

    #[test]
    fn link() {
        let text = " https://example.com/. Foobar";
        let ranges = [];
        let styled = to_styled(text, &ranges, no_mentions);

        assert_eq!(
            styled,
            " <a href=\"https://example.com/\">https://example.com/</a>. Foobar"
        );
    }

    #[test]
    fn styled_overlapping() {
        let text = "iiiiiiiiBBBBbbbb";
        let ranges = super::deserialize(&[10, 4, 16, 12, 32, 2, 10, 6, 8, 8, 16, 8, 32, 1]);
        println!("{ranges:?}");
        let styled = to_styled(text, &ranges, no_mentions);

        let possibilities = [
            "<span style='background-color: \"white\"; color: \"white\";'>iiiiiiii</span><span style='background-color: \"white\"; color: \"white\";'><i>BBBB</i></span><i>bbbb</i>",
            "<span style='background-color: \"white\"; color: \"white\";'>iiiiiiii</span><i><span style='background-color: \"white\"; color: \"white\";'>BBBB</span></i><i>bbbb</i>",
            "<span style='background-color: \"white\"; color: \"white\";'>iiiiiiii<i>BBBB</i></span><i>bbbb</i>",
            "<span style='background-color: \"white\"; color: \"white\";'>iiiiiiii</span><i><span style='background-color: \"white\"; color: \"white\";'>BBBB</span>bbbb</i>",
        ];

        assert!(possibilities.contains(&(&styled as &str)), "{}", styled);
    }

    #[test]
    fn styled_overlapping_2() {
        let text = "BSIB";
        let ranges = super::deserialize(&[
            10, 6, 8, 2, 16, 1, 32, 1, 10, 6, 8, 1, 16, 2, 32, 3, 10, 4, 16, 4, 32, 0,
        ]);
        println!("{ranges:?}");
        let styled = to_styled(text, &ranges, no_mentions);

        let possibilities = [
            "<b>B<s>S<i>I</i></s>B</b>",
            "<b>B</b><b><s>S</s></b><b><i><s>I</s></i></b><b>B</b>",
            // XXX: this is weird, but also valid. This probably means there's some empty segment in the middle, which we shouldn't generate, or at least prune.
            "<b>B</b><b><s>S</s></b><b><i><s>I</s></i></b><b><s></s></b><b>B</b>",
        ];

        assert!(possibilities.contains(&(&styled as &str)), "{}", styled);
    }

    #[test]
    // This is a regression test for a crash that happened when mentioning a user
    // https://gitlab.com/whisperfish/whisperfish/-/issues/629
    fn mention_crash() {
        use database_protos::body_range_list::body_range::Style;
        let text =
            "I ￼ am 😉  testing complex @-mentions ￼  (sorry for the crashing Whisperfishes)";
        let ranges = [
            BodyRange {
                start: 2,
                length: 1,
                associated_value: Some(AssociatedValue::MentionUuid(
                    "9bad15b5-xxxx-xxxx-xxxx-xxxxxxxxxxxx".to_string(),
                )),
            },
            BodyRange {
                start: 21,
                length: 2,
                associated_value: Some(AssociatedValue::Style(Style::Bold.into())),
            },
            BodyRange {
                start: 38,
                length: 1,
                associated_value: Some(AssociatedValue::MentionUuid(
                    "9d4428ab-xxxx-xxxx-xxxx-xxxxxxxxxxxx".to_string(),
                )),
            },
        ];
        let styled = to_styled(text, &ranges, |_u| match _u {
            "9bad15b5-xxxx-xxxx-xxxx-xxxxxxxxxxxx" => "rubdos",
            "9d4428ab-xxxx-xxxx-xxxx-xxxxxxxxxxxx" => "direc85",
            _ => panic!("unexpected mention {_u}"),
        });

        assert_eq!("I <a href=\"mention://9bad15b5-xxxx-xxxx-xxxx-xxxxxxxxxxxx\">@rubdos</a> am \u{1f609}  testing co<b>mp</b>lex @-mentions <a href=\"mention://9d4428ab-xxxx-xxxx-xxxx-xxxxxxxxxxxx\">@direc85</a>  (sorry for the crashing Whisperfishes)", styled);
    }

    #[test]
    fn mention_url_twice_crash() {
        let text = "Mention ￼ URL https://gitlab.com/ Another ￼ Link! https://github.com/";
        let ranges = [
            BodyRange {
                start: 8,
                length: 1,
                associated_value: Some(AssociatedValue::MentionUuid(
                    "89fca563-xxxx-xxxx-xxxx-xxxxxxxxxxxx".to_string(),
                )),
            },
            BodyRange {
                start: 42,
                length: 1,
                associated_value: Some(AssociatedValue::MentionUuid(
                    "9d4428ab-xxxx-xxxx-xxxx-xxxxxxxxxxxx".to_string(),
                )),
            },
        ];
        println!("{ranges:?}");
        let styled = to_styled(text, &ranges, |_u| match _u {
            "89fca563-xxxx-xxxx-xxxx-xxxxxxxxxxxx" => "foobar",
            "9d4428ab-xxxx-xxxx-xxxx-xxxxxxxxxxxx" => "direc85",
            _ => panic!("unexpected mention {_u}"),
        });

        assert_eq!("Mention <a href=\"mention://89fca563-xxxx-xxxx-xxxx-xxxxxxxxxxxx\">@foobar</a> URL <a href=\"https://gitlab.com/\">https://gitlab.com/</a> Another <a href=\"mention://9d4428ab-xxxx-xxxx-xxxx-xxxxxxxxxxxx\">@direc85</a> Link! <a href=\"https://github.com/\">https://github.com/</a>", styled);
    }

    #[test]
    fn monospace() {
        let text = "This is a monospace word.";
        let ranges = [BodyRange {
            start: 10,
            length: 9,
            associated_value: Some(AssociatedValue::Style(4)),
        }];
        println!("{ranges:?}");
        let styled = to_styled(text, &ranges, no_mentions);

        assert_eq!(styled, "This is a <pre>monospace</pre> word.");
    }

    #[test]
    fn monospace_with_newlines() {
        let text = "This is a monospace sentence\nwith line\nbreaks in it.";
        let ranges = [BodyRange {
            start: 10,
            length: 35,
            associated_value: Some(AssociatedValue::Style(4)),
        }];
        println!("{ranges:?}");
        let styled = to_styled(text, &ranges, no_mentions);

        assert_eq!(
            styled,
            "This is a <pre>monospace sentence\nwith line\nbreaks</pre> in it."
        );
    }

    #[test]
    fn monospace_with_tags() {
        let text = "This is <pre>monospace</pre> with pre tags.";
        let ranges = [BodyRange {
            start: 8,
            length: 20,
            associated_value: Some(AssociatedValue::Style(4)),
        }];
        println!("{ranges:?}");
        let styled = to_styled(text, &ranges, no_mentions);

        assert_eq!(
            styled,
            "This is <pre>&lt;pre&gt;monospace&lt;/pre&gt;</pre> with pre tags."
        );
    }

    #[test]
    fn url_in_spoiler() {
        let text =
            "Spoilers: you shouldn't see this https://localhost/if-the-bug-is-fixed nor this";
        let ranges = [BodyRange {
            start: 28,
            length: 51,
            associated_value: Some(AssociatedValue::Style(2)),
        }];
        println!("{ranges:?}");
        let styled = to_styled(text, &ranges, no_mentions);

        assert_eq!("Spoilers: you shouldn't see <span style='background-color: \"white\"; color: \"white\";'>this <a style='background-color: \"white\"; color: \"white\";' href=\"https://localhost/if-the-bug-is-fixed\">https://localhost/if-the-bug-is-fixed</a> nor this</span>", styled);
    }

    #[test]
    fn url_matches_spoiler() {
        let text = "Spoiler-link https://gitlab.com/ and that's it.";
        let ranges = [BodyRange {
            start: 13,
            length: 19,
            associated_value: Some(AssociatedValue::Style(2)),
        }];
        println!("{ranges:?}");
        let styled = to_styled(text, &ranges, no_mentions);

        assert_eq!("Spoiler-link <span style='background-color: \"white\"; color: \"white\";'><a style='background-color: \"white\"; color: \"white\";' href=\"https://gitlab.com/\">https://gitlab.com/</a></span> and that's it.", styled);
    }

    #[test]
    fn plain_with_lt_and_gt() {
        let text = "oh no :< oh yes :>";
        let ranges = [];
        println!("{ranges:?}");
        let styled = to_styled(text, &ranges, no_mentions);

        assert_eq!("oh no :&lt; oh yes :&gt;", styled);
    }

    #[test]
    fn styled_with_lt_and_gt() {
        let text = "oh no :< oh yes :>";
        let ranges = [BodyRange {
            start: 0,
            length: 5,
            associated_value: Some(AssociatedValue::Style(0)),
        }];
        println!("{ranges:?}");
        let styled = to_styled(text, &ranges, no_mentions);

        assert_eq!("<b>oh no</b> :&lt; oh yes :&gt;", styled);
    }

    #[test]
    fn kletterli_issue_minimized() {
        let text = "Bi https://www.rubdos.be/this-does-not-exists\n\nhttps://www.rubdos.be/this-does-not-exists\nSome more text that doesn't really matter anymore";

        let ranges = [
            BodyRange {
                start: 0,
                length: 2,
                associated_value: Some(AssociatedValue::Style(Style::Bold.into())),
            },
            BodyRange {
                start: 3,
                length: 44,
                associated_value: Some(AssociatedValue::Style(Style::Bold.into())),
            },
        ];

        println!("{ranges:?}");
        // This paniced
        let _styled = to_styled(text, &ranges, no_mentions);
    }
}
