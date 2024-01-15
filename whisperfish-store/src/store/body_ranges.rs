use crate::store::protos as database_protos;
pub use database_protos::body_range_list::{body_range::AssociatedValue, BodyRange};
use itertools::Itertools;
use libsignal_service::proto::{
    body_range as wire_body_range, body_range::Style as WireStyle, BodyRange as WireBodyRange,
};
use prost::Message;

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
                            let style = match WireStyle::from_i32(*style).unwrap() {
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

fn escape(s: &str) -> std::borrow::Cow<'_, str> {
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

/// Returns a styled message, with ranges for bold, italic, links, quotes, etc.
pub fn to_styled<'a, S: AsRef<str> + 'a>(
    message: &'a str,
    ranges: &'a [BodyRange],
    mention_lookup: impl Fn(&'a str) -> S,
) -> String {
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
            self.start + self.contents.len()
        }

        fn split_at(&self, idx: usize) -> [Self; 2] {
            if self.mention.is_some() {
                tracing::warn!("splitting a mention");
            }

            // Map to character boundary
            let idx = self.contents.char_indices().nth(idx).unwrap().0;

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
                    start: self.start + idx,
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
        .map(|span| Segment {
            contents: span.as_str(),
            start: span.start(),
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
        })
        .collect();

    fn annotate<'a>(segment: &'_ mut Segment<'a>, style: Option<&'a AssociatedValue>) {
        let Some(style) = style else { return };
        match style {
            AssociatedValue::Style(0) => segment.bold = true,
            AssociatedValue::Style(1) => segment.italic = true,
            AssociatedValue::Style(2) => segment.spoiler = true,
            AssociatedValue::Style(3) => segment.strikethrough = true,
            AssociatedValue::Style(4) => segment.monospace = true,
            AssociatedValue::MentionUuid(s) => segment.mention = Some(s),
            _ => {}
        }
    }

    // Every BodyRange splits one or two segments into two, and adds a style to the affected segment.
    for range in ranges {
        // XXX Just skip the range if necessary, that's healthier than panicking.
        let end = (range.start + range.length) as usize;
        assert!(end <= message.len());
        let left = segments
            .binary_search_by(|segment| {
                if segment.end() < range.start as usize {
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
        if right_split_at != segments[right].contents.len() {
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
                (segment.spoiler, "spoiler"),
                (segment.strikethrough, "s"),
                (segment.monospace, "tt"),
            ];

            for (add_tag, tag) in &tags {
                if *add_tag {
                    result.push('<');
                    result.push_str(tag);
                    result.push('>');
                }
            }
            let contents = escape(segment.contents);

            if let Some(mention) = segment.mention {
                result.push_str("<a href=\"mention://");
                result.push_str(mention);
                result.push_str("\">@");
                result.push_str(mention_lookup(mention).as_ref());
                result.push_str("</a>");
            } else if let Some(link) = segment.link {
                result.push_str("<a href=\"");
                result.push_str(link);
                result.push_str("\">");
                result.push_str(&contents);
                result.push_str("</a>");
            } else {
                result.push_str(&contents);
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
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

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
    fn mention() {
        let text = " Sorry for the random mention.";
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
            "<spoiler>iiiiiiii</spoiler><spoiler><i>BBBB</i></spoiler><i>bbbb</i>",
            "<spoiler>iiiiiiii</spoiler><i><spoiler>BBBB</spoiler></i><i>bbbb</i>",
            "<spoiler>iiiiiiii<i>BBBB</i></spoiler><i>bbbb</i>",
            "<spoiler>iiiiiiii</spoiler><i><spoiler>BBBB</spoiler>bbbb</i>",
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
        ];

        assert!(possibilities.contains(&(&styled as &str)), "{}", styled);
    }
}
