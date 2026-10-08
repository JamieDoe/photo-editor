//! XMP sidecars (ADR 0067, part 2): the photographer's marks in a `NAME.xmp` file beside
//! a RAW, where Lightroom, Bridge and Capture One look for them. RAW files are never
//! written to.
//!
//! A sidecar may already exist, written by another app with much more in it. Only the
//! rating and label are changed; everything else is kept byte for byte. A file whose
//! structure isn't recognised is left alone. A sidecar is deleted only when it is
//! exactly one this app would write with nothing in it.

use crate::metadata::{Judgements, xmp_packet};

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
const DESCRIPTION: &str = "<rdf:Description";

/// The sidecar for `judgements`, given the file there now (`existing`).
pub fn update(
    existing: Option<&str>,
    judgements: &Judgements,
) -> Result<SidecarChange, SidecarError> {
    let Some(text) = existing else {
        return Ok(match xmp_packet(judgements) {
            Some(packet) => SidecarChange::Write(packet),
            None => SidecarChange::Leave,
        });
    };
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
    if current("Rating") == rating.map(|r| r.to_string())
        && current("Label") == label.map(str::to_owned)
    {
        return Ok(SidecarChange::Leave);
    }
    let mut updated = text.to_owned();
    for property in ["Rating", "Label"] {
        updated = remove_property(&updated, pfx, property);
    }
    if rating.is_some() || label.is_some() {
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
    } else if is_empty_template(&updated) {
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

/// Whether `text` is this app's sidecar with nothing in it: the packet it writes, minus
/// its properties (whitespace aside).
fn is_empty_template(text: &str) -> bool {
    let squeeze = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let template = xmp_packet(&Judgements {
        rating: 1,
        ..Default::default()
    })
    .expect("a rating makes a packet")
    .replace(" xmp:Rating=\"1\"", "");
    squeeze(text) == squeeze(&template)
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
            update(None, &m).unwrap(),
            SidecarChange::Write(xmp_packet(&m).unwrap())
        );
        assert_eq!(
            update(None, &marks(0, false, None)).unwrap(),
            SidecarChange::Leave
        );
    }

    #[test]
    fn another_apps_sidecar_keeps_everything_but_the_marks() {
        let SidecarChange::Write(out) =
            update(Some(LIGHTROOM), &marks(5, false, Some(LabelName::Red))).unwrap()
        else {
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
        let SidecarChange::Write(cleared) = update(Some(&out), &marks(0, false, None)).unwrap()
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
        let SidecarChange::Write(out) = update(Some(text), &marks(1, false, None)).unwrap() else {
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
        let SidecarChange::Write(out) = update(Some(xap), &marks(2, false, None)).unwrap() else {
            panic!("expected a write")
        };
        assert!(
            out.contains(r#"xap:Rating="2""#)
                && !out.contains("xap:Rating='4'")
                && !out.contains("xmlns:xmp"),
            "{out}"
        );
        let none = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/"/></rdf:RDF>"#;
        let SidecarChange::Write(out) = update(Some(none), &marks(0, true, None)).unwrap() else {
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
            update(Some(&ours), &marks(0, false, None)).unwrap(),
            SidecarChange::Delete
        );
        // Unchanged marks: nothing to write.
        assert_eq!(
            update(Some(&ours), &marks(4, false, Some(LabelName::Purple))).unwrap(),
            SidecarChange::Leave
        );
    }

    #[test]
    fn marks_already_in_another_apps_sidecar_leave_it_untouched() {
        assert_eq!(
            update(Some(LIGHTROOM), &marks(2, false, Some(LabelName::Yellow))).unwrap(),
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
    fn an_unrecognised_file_is_left_alone() {
        assert_eq!(
            update(Some("not xmp at all"), &marks(3, false, None)),
            Err(SidecarError::Unrecognised)
        );
        // Nothing to write and nothing of ours in it: left as it is.
        assert_eq!(
            update(Some("not xmp at all"), &marks(0, false, None)).unwrap(),
            SidecarChange::Leave
        );
    }

    #[test]
    fn names_that_merely_contain_the_property_are_not_touched() {
        let text = r#"<rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmpMM:Rating="9" myxmp:Rating="8" xmp:Rating="3"/>"#;
        let SidecarChange::Write(out) = update(Some(text), &marks(1, false, None)).unwrap() else {
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
}
