//! Encoders for Content and the legacy ContentId. Mirrors `value::content`.
//! Studio saves both under a `Content` tag, told apart by the child: `uri` for
//! a Content, `url` for a ContentId, `null` for either when unset. The legacy
//! `ContentId` tag itself is never written.

use rbx_dom::Content;

use crate::serializer::referent;
use crate::serializer::writer::Writer;

pub(crate) fn content(writer: &mut Writer, name: &str, value: &Content) {
    writer.open("Content", &[("name", name)]);
    match value {
        Content::None => writer.leaf("null", &[], ""),
        Content::Uri(uri) => writer.leaf("uri", &[], uri),
        Content::Object(target) => writer.leaf("Ref", &[], &referent(*target)),
    }
    writer.close("Content");
}

/// A legacy ContentId (`Decal.Texture`, `ImageLabel.Image`), the way Studio
/// saves one: its URI as a `url` child, or `null` when empty.
pub(crate) fn content_id(writer: &mut Writer, name: &str, uri: &str) {
    writer.open("Content", &[("name", name)]);
    match uri {
        "" => writer.leaf("null", &[], ""),
        _ => writer.leaf("url", &[], uri),
    }
    writer.close("Content");
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbx_dom::Ref;

    #[test]
    fn object_writes_a_ref_child() {
        let mut writer = Writer::new();
        content(&mut writer, "X", &Content::Object(Ref::new(7)));
        assert!(writer.into_string().contains("<Ref>RBX7</Ref>"));
    }

    #[test]
    fn content_id_writes_a_url_child_or_null_when_empty() {
        let mut writer = Writer::new();
        content_id(&mut writer, "Texture", "rbxassetid://5");
        content_id(&mut writer, "Image", "");
        assert_eq!(
            writer.into_string(),
            "<Content name=\"Texture\">\n  <url>rbxassetid://5</url>\n</Content>\n\
             <Content name=\"Image\">\n  <null></null>\n</Content>\n"
        );
    }

    #[test]
    fn none_writes_a_null_child() {
        let mut writer = Writer::new();
        content(&mut writer, "X", &Content::None);
        assert!(writer.into_string().contains("<null></null>"));
    }
}
