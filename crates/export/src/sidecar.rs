//! XMP sidecars (ADR 0067, part 2): the photographer's marks and keywords (ADR 0088)
//! in a `NAME.xmp` file beside a RAW, where Lightroom, Bridge and Capture One look for
//! them. RAW files are never written to.
//!
//! A sidecar may already exist, written by another app with much more in it. Only the
//! rating, label and keywords are changed; everything else is kept byte for byte. A
//! file whose structure isn't recognised is left alone. A sidecar is deleted only when
//! it is exactly one this app would write with nothing in it.

use crate::metadata::{DC_NAMESPACE, Judgements, LabelName, subject_element, xmp_packet};

/// Whether a sidecar's keywords are made to match the photo's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeywordSync {
    /// Left as they are: the photo has none here, and the sidecar may hold another
    /// app's that were never brought in.
    Leave,
    /// Made exactly the photo's: they were just changed here, or the photo has some.
    Match,
}

/// What to do with a sidecar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarChange {
    /// Write this text (a new sidecar, or the existing one updated).
    Write(String),
    /// Remove it: one of ours, now with nothing in it.
    Delete,
    /// Leave it as it is (already right, or nothing to write).
    Leave,
}

/// Why an existing sidecar can't be updated safely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarError {
    /// No `rdf:Description` element to hold the marks.
    Unrecognised,
}

impl std::fmt::Display for SidecarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unrecognised => write!(f, "sidecar has no rdf:Description to update"),
        }
    }
}

impl std::error::Error for SidecarError {}

const XMP_NAMESPACE: &str = "http://ns.adobe.com/xap/1.0/";
/// Lightroom's namespace, whose `hierarchicalSubject` holds keywords with their parents.
const LIGHTROOM_NAMESPACE: &str = "http://ns.adobe.com/lightroom/1.0/";
const DESCRIPTION: &str = "<rdf:Description";

/// The sidecar for `judgements`, given the file there now (`existing`).
pub fn update(
    existing: Option<&str>,
    judgements: &Judgements,
    keywords: KeywordSync,
) -> Result<SidecarChange, SidecarError> {
    let Some(text) = existing else {
        return Ok(match xmp_packet(judgements) {
            Some(packet) => SidecarChange::Write(packet),
            None => SidecarChange::Leave,
        });
    };
    let keywords_right =
        keywords == KeywordSync::Leave || same_keywords(&read_keywords(text), &judgements.keywords);
    // The prefix the file binds to the XMP namespace (Lightroom's is "xmp"; older files
    // may use "xap"), or none yet.
    let prefix = namespace_prefix(text, XMP_NAMESPACE);
    let pfx = prefix.as_deref().unwrap_or("xmp");
    let (rating, label) = values(judgements);
    // Already right: left untouched, so another app's file isn't rewritten for nothing.
    let current = |property| {
        prefix
            .as_deref()
            .and_then(|p| read_property(text, p, property))
    };
    let marks_right = current("Rating") == rating.map(|r| r.to_string())
        && current("Label") == label.map(str::to_owned);
    if marks_right && keywords_right {
        return Ok(SidecarChange::Leave);
    }
    let mut updated = text.to_owned();
    if !keywords_right {
        updated = replace_keywords(&updated, &judgements.keywords)?;
    }
    if marks_right {
        // Only the keywords changed.
    } else if rating.is_some() || label.is_some() {
        for property in ["Rating", "Label"] {
            updated = remove_property(&updated, pfx, property);
        }
        let at = updated
            .find(DESCRIPTION)
            .ok_or(SidecarError::Unrecognised)?
            + DESCRIPTION.len();
        let mut attributes = String::new();
        if prefix.is_none() {
            attributes.push_str(&format!(" xmlns:xmp=\"{XMP_NAMESPACE}\""));
        }
        if let Some(r) = rating {
            attributes.push_str(&format!(" {pfx}:Rating=\"{r}\""));
        }
        if let Some(l) = label {
            attributes.push_str(&format!(" {pfx}:Label=\"{l}\""));
        }
        updated.insert_str(at, &attributes);
    } else {
        for property in ["Rating", "Label"] {
            updated = remove_property(&updated, pfx, property);
        }
    }
    if is_empty_template(&updated) {
        return Ok(SidecarChange::Delete);
    }
    Ok(if updated == text {
        SidecarChange::Leave
    } else {
        SidecarChange::Write(updated)
    })
}

/// The rating (a reject as -1) and label to write, as `xmp_packet` writes them.
fn values(j: &Judgements) -> (Option<i32>, Option<&'static str>) {
    let rating = if j.rejected {
        Some(-1)
    } else {
        (j.rating > 0).then(|| i32::from(j.rating.min(5)))
    };
    (rating, j.label.map(|l| l.as_str()))
}

/// Whether two keyword lists hold the same keywords, in any order.
fn same_keywords(a: &[String], b: &[String]) -> bool {
    let sorted = |k: &[String]| {
        let mut k = k.to_vec();
        k.sort();
        k
    };
    sorted(a) == sorted(b)
}

/// The keywords in `text` (`dc:subject`), as another app or this one wrote them; none
/// when it has none or doesn't declare the Dublin Core namespace.
pub fn read_keywords(text: &str) -> Vec<String> {
    let Some(dc) = namespace_prefix(text, DC_NAMESPACE) else {
        return Vec::new();
    };
    element_content(text, &format!("{dc}:subject"))
        .map(|bag| {
            list_items(bag)
                .into_iter()
                .map(|(_, value)| value)
                .collect()
        })
        .unwrap_or_default()
}

/// The content of the first `<name>` element in `text` (attributes allowed), if any.
fn element_content<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let range = element_range(text, name)?;
    let element = &text[range];
    let start = start_tag_end(element, 0)? + 1;
    let end = element.rfind(&format!("</{name}>"))?;
    Some(&element[start..end])
}

/// Where the first `<name ...>...</name>` element in `text` is, if it has one.
fn element_range(text: &str, name: &str) -> Option<std::ops::Range<usize>> {
    let open = format!("<{name}");
    let mut from = 0;
    while let Some(i) = text[from..].find(&open) {
        let at = from + i;
        // `<dc:subject>` or `<dc:subject attr=…>`, not `<dc:subjects>`.
        if matches!(
            text[at + open.len()..].chars().next(),
            Some('>' | ' ' | '\t' | '\n' | '\r')
        ) {
            let close = format!("</{name}>");
            let end = text[at..].find(&close)? + at + close.len();
            return Some(at..end);
        }
        from = at + open.len();
    }
    None
}

/// The `<rdf:li>` items in `text`: each one's whole range and its unescaped value.
fn list_items(text: &str) -> Vec<(std::ops::Range<usize>, String)> {
    let mut items = Vec::new();
    let mut from = 0;
    while let Some(i) = text[from..].find("<rdf:li") {
        let at = from + i;
        let Some(open_end) = start_tag_end(text, at) else {
            break;
        };
        if text[..open_end].ends_with('/') {
            from = open_end + 1;
            continue;
        }
        let Some(close) = text[open_end..].find("</rdf:li>") else {
            break;
        };
        let end = open_end + close + "</rdf:li>".len();
        let value = unescape_xml(text[open_end + 1..open_end + close].trim());
        if !value.is_empty() {
            items.push((at..end, value));
        }
        from = end;
    }
    items
}

/// Where the start tag beginning at `at` ends: its `>`, outside quoted values.
fn start_tag_end(text: &str, at: usize) -> Option<usize> {
    let mut quote = None;
    for (i, c) in text[at..].char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return Some(at + i),
            _ => {}
        }
    }
    None
}

/// `text` with XML's character references and predefined entities replaced.
fn unescape_xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|&e| e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let ch = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .map(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| entity.strip_prefix('#').map(|d| d.parse().ok()))
                .flatten()
                .and_then(char::from_u32),
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// `text` with its keywords made `keywords`: the `dc:subject` element replaced (or
/// added to the first description, or removed), and Lightroom's keyword hierarchy
/// trimmed to the keywords still there (it can't be extended: the parents aren't known
/// here).
fn replace_keywords(text: &str, keywords: &[String]) -> Result<String, SidecarError> {
    let mut out = text.to_owned();
    let dc = namespace_prefix(&out, DC_NAMESPACE);
    if let Some(dc) = &dc {
        while let Some(range) = element_range(&out, &format!("{dc}:subject")) {
            out.replace_range(whole_lines(&out, range), "");
        }
    }
    out = prune_hierarchy(&out, keywords);
    if keywords.is_empty() {
        return Ok(out);
    }
    let at = out.find(DESCRIPTION).ok_or(SidecarError::Unrecognised)?;
    let tag_end = start_tag_end(&out, at).ok_or(SidecarError::Unrecognised)?;
    let line_start = out[..at].rfind('\n').map_or(0, |i| i + 1);
    let outer: String = out[line_start..at]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let indent = format!("{outer} ");
    let declaration = match &dc {
        Some(_) => String::new(),
        None => format!(" xmlns:dc=\"{DC_NAMESPACE}\""),
    };
    let element = subject_element(dc.as_deref().unwrap_or("dc"), keywords, &indent);
    let slash = out[..tag_end]
        .trim_end()
        .strip_suffix('/')
        .map(|before| before.trim_end().len());
    match slash {
        // `<rdf:Description …/>`: opened up to hold the element.
        Some(slash_at) => out.replace_range(
            slash_at..=tag_end,
            &format!("{declaration}>\n{indent}{element}\n{outer}</rdf:Description>"),
        ),
        None => out.replace_range(
            tag_end..=tag_end,
            &format!("{declaration}>\n{indent}{element}"),
        ),
    }
    Ok(out)
}

/// `range` widened to its whole lines when nothing else is on them: the indentation
/// before and the line break after.
fn whole_lines(text: &str, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
    let mut start = range.start;
    while start > 0 && matches!(text.as_bytes()[start - 1], b' ' | b'\t') {
        start -= 1;
    }
    let at_line_start = start == 0 || text.as_bytes()[start - 1] == b'\n';
    let mut end = range.end;
    if at_line_start {
        if text[end..].starts_with("\r\n") {
            end += 2;
        } else if text[end..].starts_with('\n') {
            end += 1;
        }
    } else {
        start = range.start;
    }
    start..end
}

/// `text` with Lightroom's `hierarchicalSubject` ("Animals|Birds|Heron") keeping only
/// the paths whose last keyword is still one of `keywords`, and gone when none is.
fn prune_hierarchy(text: &str, keywords: &[String]) -> String {
    let Some(lr) = namespace_prefix(text, LIGHTROOM_NAMESPACE) else {
        return text.to_owned();
    };
    let name = format!("{lr}:hierarchicalSubject");
    let Some(range) = element_range(text, &name) else {
        return text.to_owned();
    };
    let element = &text[range.clone()];
    let items = list_items(element);
    let kept = |path: &str| {
        let leaf = path.rsplit('|').next().unwrap_or(path).trim();
        keywords.iter().any(|k| k == leaf)
    };
    let mut out = text.to_owned();
    if !items.iter().any(|(_, path)| kept(path)) {
        out.replace_range(whole_lines(text, range), "");
        return out;
    }
    // From the last, so earlier ranges stay put.
    for (item, path) in items.iter().rev() {
        if !kept(path) {
            let absolute = range.start + item.start..range.start + item.end;
            let cut = whole_lines(&out, absolute);
            out.replace_range(cut, "");
        }
    }
    out
}

/// Whether `text` is this app's sidecar with nothing in it: the packet it writes, minus
/// its properties (whitespace aside), including one that held keywords.
fn is_empty_template(text: &str) -> bool {
    let squeeze = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let text = squeeze(text)
        .replace(&format!("xmlns:dc=\"{DC_NAMESPACE}\""), "")
        .replace("></rdf:Description>", "/>");
    let template = xmp_packet(&Judgements {
        rating: 1,
        ..Default::default()
    })
    .expect("a rating makes a packet")
    .replace(" xmp:Rating=\"1\"", "");
    text == squeeze(&template)
}

/// The prefix bound to `namespace` by an `xmlns:prefix="namespace"` declaration.
fn namespace_prefix(text: &str, namespace: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let needle = format!("={quote}{namespace}{quote}");
        let mut from = 0;
        while let Some(i) = text[from..].find(&needle) {
            let end = from + i;
            let before = &text[..end];
            if let Some(start) = before.rfind("xmlns:") {
                let name = &before[start + "xmlns:".len()..];
                if !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                {
                    return Some(name.to_owned());
                }
            }
            from = end + needle.len();
        }
    }
    None
}

/// `text` without `prefix:property`, written either as an attribute
/// (` xmp:Rating="3"`) or as an element (`<xmp:Rating>3</xmp:Rating>`).
fn remove_property(text: &str, prefix: &str, property: &str) -> String {
    let name = format!("{prefix}:{property}");
    let mut out = text.to_owned();
    // Attributes: whitespace, the name, `=`, then a quoted value.
    while let Some((start, value_start)) = find_attribute(&out, &name) {
        let quote = out[value_start..].chars().next().unwrap_or('"');
        let Some(close) = out[value_start + 1..].find(quote) else {
            break;
        };
        out.replace_range(start..value_start + 1 + close + 1, "");
    }
    // Elements.
    let (open, close) = (format!("<{name}>"), format!("</{name}>"));
    while let Some(start) = out.find(&open) {
        let Some(end) = out[start..].find(&close) else {
            break;
        };
        let mut cut_from = start;
        // Take the line's indentation with it.
        while cut_from > 0 && matches!(out.as_bytes()[cut_from - 1], b' ' | b'\t') {
            cut_from -= 1;
        }
        let mut cut_to = start + end + close.len();
        if out[cut_to..].starts_with("\r\n") {
            cut_to += 2;
        } else if out[cut_to..].starts_with('\n') {
            cut_to += 1;
        }
        out.replace_range(cut_from..cut_to, "");
    }
    out
}

/// The marks a sidecar holds (ADR 0067, part 3), as another app wrote them: the
/// rating (-1 meaning rejected) and the colour label by its English name, any case.
/// `None` when it holds neither, or the XMP namespace isn't declared. Labels renamed
/// in another app (custom label sets) aren't recognised and are left out.
pub fn read_marks(text: &str) -> Option<Judgements> {
    let prefix = namespace_prefix(text, XMP_NAMESPACE)?;
    let mut marks = Judgements::default();
    match read_property(text, &prefix, "Rating").and_then(|r| r.trim().parse::<i32>().ok()) {
        Some(-1) => marks.rejected = true,
        Some(r @ 1..=5) => marks.rating = r as u8,
        _ => {}
    }
    marks.label = read_property(text, &prefix, "Label").and_then(|l| {
        LabelName::ALL
            .into_iter()
            .find(|name| name.as_str().eq_ignore_ascii_case(l.trim()))
    });
    (marks != Judgements::default()).then_some(marks)
}

/// The value of `prefix:property` in `text`, written as an attribute or an element.
pub fn read_property(text: &str, prefix: &str, property: &str) -> Option<String> {
    let name = format!("{prefix}:{property}");
    if let Some((_, quote_at)) = find_attribute(text, &name) {
        let quote = text[quote_at..].chars().next()?;
        let value = &text[quote_at + 1..];
        return value.find(quote).map(|end| value[..end].to_owned());
    }
    let open = format!("<{name}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&format!("</{name}>"))?;
    Some(text[start..start + end].trim().to_owned())
}

/// Where attribute `name` starts (its leading whitespace) and where its value's quote
/// is, if `text` has it.
fn find_attribute(text: &str, name: &str) -> Option<(usize, usize)> {
    let mut from = 0;
    while let Some(i) = text[from..].find(name) {
        let at = from + i;
        let after = &text[at + name.len()..];
        let preceded = text[..at].chars().last().is_some_and(char::is_whitespace);
        let trimmed = after.trim_start();
        if preceded && trimmed.starts_with('=') {
            let eq = at + name.len() + (after.len() - trimmed.len());
            let value = &text[eq + 1..];
            let quote_at = eq + 1 + (value.len() - value.trim_start().len());
            if matches!(text[quote_at..].chars().next(), Some('"' | '\'')) {
                let start = text[..at].trim_end().len();
                return Some((start, quote_at));
            }
        }
        from = at + name.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::LabelName;

    fn marks(rating: u8, rejected: bool, label: Option<LabelName>) -> Judgements {
        Judgements {
            rating,
            rejected,
            label,
            keywords: Vec::new(),
            description: Default::default(),
        }
    }

    /// A sidecar as Lightroom Classic writes one, trimmed: attributes on the
    /// description, other namespaces and content around them.
    const LIGHTROOM: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="Adobe XMP Core 7.0-c000">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
   xmp:Rating="2"
   xmp:Label="Yellow"
   xmp:CreatorTool="Adobe Photoshop Lightroom Classic 13.0"
   crs:Exposure2012="+0.35"
   crs:Contrast2012="+12">
   <crs:ToneCurvePV2012>
    <rdf:Seq>
     <rdf:li>0, 0</rdf:li>
     <rdf:li>255, 255</rdf:li>
    </rdf:Seq>
   </crs:ToneCurvePV2012>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
"#;

    #[test]
    fn a_new_sidecar_is_the_packet_exports_carry() {
        let m = marks(3, false, Some(LabelName::Green));
        assert_eq!(
            update(None, &m, KeywordSync::Leave).unwrap(),
            SidecarChange::Write(xmp_packet(&m).unwrap())
        );
        assert_eq!(
            update(None, &marks(0, false, None), KeywordSync::Leave).unwrap(),
            SidecarChange::Leave
        );
    }

    #[test]
    fn another_apps_sidecar_keeps_everything_but_the_marks() {
        let SidecarChange::Write(out) = update(
            Some(LIGHTROOM),
            &marks(5, false, Some(LabelName::Red)),
            KeywordSync::Leave,
        )
        .unwrap() else {
            panic!("expected a write")
        };
        assert!(
            out.contains(r#"xmp:Rating="5""#) && out.contains(r#"xmp:Label="Red""#),
            "{out}"
        );
        assert!(
            !out.contains(r#"xmp:Rating="2""#) && !out.contains("Yellow"),
            "{out}"
        );
        // Everything else exactly as it was.
        for kept in [
            r#"xmp:CreatorTool="Adobe Photoshop Lightroom Classic 13.0""#,
            r#"crs:Exposure2012="+0.35""#,
            "<rdf:li>255, 255</rdf:li>",
            r#"x:xmptk="Adobe XMP Core 7.0-c000""#,
        ] {
            assert!(out.contains(kept), "lost {kept}");
        }
        assert_eq!(
            out.matches(r#"xmlns:xmp="#).count(),
            1,
            "no second declaration"
        );
        // Clearing the marks takes only them away; the file stays (it isn't ours).
        let SidecarChange::Write(cleared) =
            update(Some(&out), &marks(0, false, None), KeywordSync::Leave).unwrap()
        else {
            panic!("expected a write")
        };
        assert!(
            !cleared.contains("xmp:Rating") && !cleared.contains("xmp:Label"),
            "{cleared}"
        );
        assert!(cleared.contains(r#"crs:Exposure2012="+0.35""#));
    }

    #[test]
    fn marks_written_as_elements_are_replaced_too() {
        let text = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n  <rdf:Description rdf:about=\"\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\">\n   <xmp:Rating>3</xmp:Rating>\n   <xmp:Label>Blue</xmp:Label>\n   <xmp:ModifyDate>2024-01-01</xmp:ModifyDate>\n  </rdf:Description>\n </rdf:RDF>\n</x:xmpmeta>";
        let SidecarChange::Write(out) =
            update(Some(text), &marks(1, false, None), KeywordSync::Leave).unwrap()
        else {
            panic!("expected a write")
        };
        assert!(
            !out.contains("<xmp:Rating>") && !out.contains("Blue"),
            "{out}"
        );
        assert!(
            out.contains(r#"xmp:Rating="1""#)
                && out.contains("<xmp:ModifyDate>2024-01-01</xmp:ModifyDate>"),
            "{out}"
        );
    }

    #[test]
    fn an_older_prefix_is_used_and_a_missing_one_declared() {
        let xap = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:xap='http://ns.adobe.com/xap/1.0/' xap:Rating='4'/></rdf:RDF>"#;
        let SidecarChange::Write(out) =
            update(Some(xap), &marks(2, false, None), KeywordSync::Leave).unwrap()
        else {
            panic!("expected a write")
        };
        assert!(
            out.contains(r#"xap:Rating="2""#)
                && !out.contains("xap:Rating='4'")
                && !out.contains("xmlns:xmp"),
            "{out}"
        );
        let none = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/"/></rdf:RDF>"#;
        let SidecarChange::Write(out) =
            update(Some(none), &marks(0, true, None), KeywordSync::Leave).unwrap()
        else {
            panic!("expected a write")
        };
        assert!(
            out.contains(r#"xmlns:xmp="http://ns.adobe.com/xap/1.0/""#)
                && out.contains(r#"xmp:Rating="-1""#),
            "{out}"
        );
        assert!(out.contains(r#"xmlns:dc="http://purl.org/dc/elements/1.1/""#));
    }

    #[test]
    fn our_own_sidecar_is_deleted_when_its_marks_are_cleared() {
        let ours = xmp_packet(&marks(4, false, Some(LabelName::Purple))).unwrap();
        assert_eq!(
            update(Some(&ours), &marks(0, false, None), KeywordSync::Leave).unwrap(),
            SidecarChange::Delete
        );
        // Unchanged marks: nothing to write.
        assert_eq!(
            update(
                Some(&ours),
                &marks(4, false, Some(LabelName::Purple)),
                KeywordSync::Leave
            )
            .unwrap(),
            SidecarChange::Leave
        );
    }

    #[test]
    fn marks_already_in_another_apps_sidecar_leave_it_untouched() {
        assert_eq!(
            update(
                Some(LIGHTROOM),
                &marks(2, false, Some(LabelName::Yellow)),
                KeywordSync::Leave
            )
            .unwrap(),
            SidecarChange::Leave
        );
        assert_eq!(
            read_property(LIGHTROOM, "xmp", "Rating").as_deref(),
            Some("2")
        );
        assert_eq!(
            read_property(LIGHTROOM, "xmp", "Label").as_deref(),
            Some("Yellow")
        );
        assert_eq!(read_property(LIGHTROOM, "xmp", "Nothing"), None);
    }

    #[test]
    fn marks_are_read_from_another_apps_sidecar() {
        assert_eq!(
            read_marks(LIGHTROOM),
            Some(marks(2, false, Some(LabelName::Yellow)))
        );
        // Elements, an older prefix, any case of the label.
        let elements = "<rdf:Description xmlns:xap=\"http://ns.adobe.com/xap/1.0/\"><xap:Rating>5</xap:Rating><xap:Label>purple</xap:Label></rdf:Description>";
        assert_eq!(
            read_marks(elements),
            Some(marks(5, false, Some(LabelName::Purple)))
        );
        // A reject; a label alone.
        let reject =
            r#"<rdf:Description xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="-1"/>"#;
        assert_eq!(read_marks(reject), Some(marks(0, true, None)));
        let label =
            r#"<rdf:Description xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Label="Green"/>"#;
        assert_eq!(
            read_marks(label),
            Some(marks(0, false, Some(LabelName::Green)))
        );
        // Nothing to take: unrated, an unknown label, out-of-range ratings, no namespace.
        for text in [
            r#"<rdf:Description xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="0"/>"#,
            r#"<rdf:Description xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Label="To Print"/>"#,
            r#"<rdf:Description xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="7"/>"#,
            r#"<rdf:Description xmp:Rating="3"/>"#,
            "not xmp",
        ] {
            assert_eq!(read_marks(text), None, "{text}");
        }
        // What this app writes, it reads back.
        let m = marks(4, false, Some(LabelName::Blue));
        assert_eq!(read_marks(&xmp_packet(&m).unwrap()), Some(m));
    }

    #[test]
    fn an_unrecognised_file_is_left_alone() {
        assert_eq!(
            update(
                Some("not xmp at all"),
                &marks(3, false, None),
                KeywordSync::Leave
            ),
            Err(SidecarError::Unrecognised)
        );
        // Nothing to write and nothing of ours in it: left as it is.
        assert_eq!(
            update(
                Some("not xmp at all"),
                &marks(0, false, None),
                KeywordSync::Leave
            )
            .unwrap(),
            SidecarChange::Leave
        );
    }

    #[test]
    fn names_that_merely_contain_the_property_are_not_touched() {
        let text = r#"<rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmpMM:Rating="9" myxmp:Rating="8" xmp:Rating="3"/>"#;
        let SidecarChange::Write(out) =
            update(Some(text), &marks(1, false, None), KeywordSync::Leave).unwrap()
        else {
            panic!("expected a write")
        };
        assert!(
            out.contains(r#"xmpMM:Rating="9""#) && out.contains(r#"myxmp:Rating="8""#),
            "{out}"
        );
        assert!(
            out.contains(r#"xmp:Rating="1""#) && !out.contains(r#"xmp:Rating="3""#),
            "{out}"
        );
    }

    fn with_keywords(m: Judgements, keywords: &[&str]) -> Judgements {
        Judgements {
            keywords: keywords.iter().map(|k| (*k).to_owned()).collect(),
            ..m
        }
    }

    /// Lightroom Classic's sidecar with keywords: the flat list and the hierarchy.
    const LIGHTROOM_KEYWORDS: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:lr="http://ns.adobe.com/lightroom/1.0/"
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
   xmp:Rating="3"
   crs:Exposure2012="+0.35">
   <dc:subject>
    <rdf:Bag>
     <rdf:li>Heron</rdf:li>
     <rdf:li>Scotland</rdf:li>
     <rdf:li>Fish &amp; chips</rdf:li>
    </rdf:Bag>
   </dc:subject>
   <lr:hierarchicalSubject>
    <rdf:Bag>
     <rdf:li>Animals|Birds|Heron</rdf:li>
     <rdf:li>Places|Scotland</rdf:li>
    </rdf:Bag>
   </lr:hierarchicalSubject>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
"#;

    #[test]
    fn keywords_are_read_from_another_apps_sidecar() {
        assert_eq!(
            read_keywords(LIGHTROOM_KEYWORDS),
            vec!["Heron", "Scotland", "Fish & chips"]
        );
        assert!(read_keywords(LIGHTROOM).is_empty());
        // No Dublin Core namespace declared: not keywords.
        assert!(
            read_keywords("<dc:subject><rdf:Bag><rdf:li>A</rdf:li></rdf:Bag></dc:subject>")
                .is_empty()
        );
        // A different prefix and character references.
        let other = r#"<rdf:Description xmlns:d="http://purl.org/dc/elements/1.1/"><d:subject><rdf:Bag><rdf:li>Caf&#233;</rdf:li><rdf:li> </rdf:li><rdf:li/></rdf:Bag></d:subject></rdf:Description>"#;
        assert_eq!(read_keywords(other), vec!["Café"]);
    }

    #[test]
    fn keywords_in_another_apps_sidecar_are_replaced_and_the_rest_kept() {
        let m = with_keywords(marks(3, false, None), &["Heron", "Lake"]);
        let SidecarChange::Write(out) =
            update(Some(LIGHTROOM_KEYWORDS), &m, KeywordSync::Match).unwrap()
        else {
            panic!("expected a write")
        };
        assert_eq!(read_keywords(&out), vec!["Heron", "Lake"], "{out}");
        assert_eq!(out.matches("<dc:subject>").count(), 1, "{out}");
        // The hierarchy keeps only what is still a keyword; it can't gain "Lake".
        assert!(
            out.contains("<rdf:li>Animals|Birds|Heron</rdf:li>"),
            "{out}"
        );
        assert!(
            !out.contains("Places|Scotland") && !out.contains("Fish"),
            "{out}"
        );
        for kept in [
            r#"xmp:Rating="3""#,
            r#"crs:Exposure2012="+0.35""#,
            r#"xmlns:lr="#,
        ] {
            assert!(out.contains(kept), "lost {kept}: {out}");
        }
        assert_eq!(out.matches("xmlns:dc=").count(), 1, "{out}");
        // Already matching: left alone.
        assert_eq!(
            update(Some(&out), &m, KeywordSync::Match).unwrap(),
            SidecarChange::Leave
        );
        // All removed: the subject and the hierarchy go; the file stays (it isn't ours).
        let SidecarChange::Write(none) =
            update(Some(&out), &marks(3, false, None), KeywordSync::Match).unwrap()
        else {
            panic!("expected a write")
        };
        assert!(
            !none.contains("subject") && !none.contains("hierarchicalSubject"),
            "{none}"
        );
        assert!(none.contains(r#"crs:Exposure2012="+0.35""#), "{none}");
    }

    #[test]
    fn keywords_are_left_alone_unless_they_are_to_match() {
        // A rating changed on a photo with no keywords here: Lightroom's stay.
        let SidecarChange::Write(out) = update(
            Some(LIGHTROOM_KEYWORDS),
            &marks(5, false, None),
            KeywordSync::Leave,
        )
        .unwrap() else {
            panic!("expected a write")
        };
        assert!(out.contains(r#"xmp:Rating="5""#), "{out}");
        assert_eq!(
            read_keywords(&out),
            vec!["Heron", "Scotland", "Fish & chips"]
        );
        assert!(out.contains("Places|Scotland"));
    }

    #[test]
    fn a_self_closing_description_is_opened_for_keywords() {
        let text = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="2" />
 </rdf:RDF>
</x:xmpmeta>"#;
        let m = with_keywords(marks(2, false, None), &["Snow"]);
        let SidecarChange::Write(out) = update(Some(text), &m, KeywordSync::Match).unwrap() else {
            panic!("expected a write")
        };
        assert_eq!(read_keywords(&out), vec!["Snow"], "{out}");
        assert!(
            out.contains(r#"xmlns:dc="http://purl.org/dc/elements/1.1/""#),
            "{out}"
        );
        assert!(
            out.contains("</rdf:Description>") && !out.contains("/>\n"),
            "{out}"
        );
        assert!(out.contains(r#"xmp:Rating="2""#), "{out}");
    }

    #[test]
    fn our_own_sidecar_with_keywords_is_deleted_when_everything_is_cleared() {
        let m = with_keywords(marks(4, false, None), &["Heron"]);
        let ours = xmp_packet(&m).unwrap();
        assert_eq!(
            update(None, &m, KeywordSync::Match).unwrap(),
            SidecarChange::Write(ours.clone())
        );
        assert_eq!(
            update(Some(&ours), &m, KeywordSync::Match).unwrap(),
            SidecarChange::Leave
        );
        // The keywords go first, then the rating: then the file goes.
        let SidecarChange::Write(rated) =
            update(Some(&ours), &marks(4, false, None), KeywordSync::Match).unwrap()
        else {
            panic!("expected a write")
        };
        assert!(read_keywords(&rated).is_empty(), "{rated}");
        assert_eq!(
            update(Some(&rated), &marks(0, false, None), KeywordSync::Match).unwrap(),
            SidecarChange::Delete
        );
        // Both at once.
        assert_eq!(
            update(Some(&ours), &marks(0, false, None), KeywordSync::Match).unwrap(),
            SidecarChange::Delete
        );
    }
}
