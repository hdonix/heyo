//! IBus's objects as they cross D-Bus.
//!
//! IBus sends its own objects (a text with its attributes, the description
//! of an engine, a component) as a variant holding a structure: the name of
//! the object's type, a dictionary of attachments (always empty here), and
//! then the fields in the order libibus writes them. That order is the
//! format: `ibus_text_serialize` and its siblings in IBus's source are what
//! this follows.

use std::collections::HashMap;

use zbus::zvariant::{Structure, StructureBuilder, Value};

/// How IBus marks text: a line under it, which is how text being composed
/// is told from text that is the document's.
const UNDERLINE: u32 = 1;
const SINGLE: u32 = 1;

/// The start of every object: its type's name and no attachments.
fn object(kind: &'static str) -> StructureBuilder<'static> {
    StructureBuilder::new().add_field(kind).add_field(HashMap::<String, Value<'static>>::new())
}

fn built(builder: StructureBuilder<'static>) -> Value<'static> {
    // Only a structure without fields is refused, and none here is.
    Value::from(builder.build().expect("an IBus object has fields"))
}

/// An `IBusText`: `text`, underlined from end to end if it is being
/// composed. Positions count characters.
pub(super) fn text(text: &str, composed: bool) -> Value<'static> {
    let length = text.chars().count() as u32;
    let mut attributes = Vec::new();
    if composed && length > 0 {
        attributes.push(built(object("IBusAttribute").add_field(UNDERLINE).add_field(SINGLE).add_field(0u32).add_field(length)));
    }
    let attributes = built(object("IBusAttrList").add_field(attributes));
    built(object("IBusText").add_field(text.to_owned()).add_field(attributes))
}

/// An `IBusEngineDesc` for an engine that leaves the keyboard layout as it
/// is (`default`) and is offered to nobody (no language, the lowest rank).
fn engine(name: &str, title: &str) -> Value<'static> {
    let mut desc = object("IBusEngineDesc").add_field(name.to_owned()).add_field(title.to_owned());
    // description, language, license, author, icon
    for field in ["", "other", "GPL-3.0-only", "", ""] {
        desc = desc.add_field(field);
    }
    desc = desc.add_field("default").add_field(0u32);
    // hotkeys, symbol, setup, layout variant, layout option, version, text domain, icon property
    for _ in 0..8 {
        desc = desc.add_field("");
    }
    built(desc)
}

/// An `IBusComponent` with one engine and no program to start: whoever
/// registers it is the one that serves it.
pub(super) fn component(name: &str, engine_name: &str, title: &str) -> Value<'static> {
    let mut component = object("IBusComponent").add_field(name.to_owned()).add_field(title.to_owned());
    // version, license, author, homepage, command line, text domain
    for field in [env!("CARGO_PKG_VERSION"), "GPL-3.0-only", "", "", "", ""] {
        component = component.add_field(field);
    }
    built(component.add_field(Vec::<Value<'static>>::new()).add_field(vec![engine(engine_name, title)]))
}

/// The name in an `IBusEngineDesc`, which is its first field of its own.
pub(super) fn engine_name(desc: &Value<'_>) -> Option<String> {
    let desc = match desc {
        Value::Value(inner) => inner.as_ref(),
        other => other,
    };
    let fields = <&Structure>::try_from(desc).ok()?.fields();
    match (fields.first()?, fields.get(2)?) {
        (Value::Str(kind), Value::Str(name)) if kind.as_str() == "IBusEngineDesc" => Some(name.as_str().to_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signature(value: &Value<'_>) -> String {
        value.value_signature().to_string()
    }

    #[test]
    fn objects_have_the_shape_libibus_reads() {
        assert_eq!(signature(&text("hello", false)), "(sa{sv}sv)");
        assert_eq!(signature(&engine("heyo", "Heyo")), "(sa{sv}ssssssssussssssss)");
        assert_eq!(signature(&component("org.freedesktop.IBus.Heyo", "heyo", "Heyo")), "(sa{sv}ssssssssavav)");
    }

    #[test]
    fn text_being_composed_is_underlined_from_end_to_end_by_characters() {
        // Each attribute as kind, value, from, to.
        let attributes = |value: &Value<'_>| -> Vec<Vec<u32>> {
            let Value::Structure(text) = value else { panic!("not a structure") };
            let Value::Value(list) = &text.fields()[3] else { panic!("no attributes") };
            let Value::Structure(list) = list.as_ref() else { panic!("not a list") };
            let Value::Array(all) = &list.fields()[2] else { panic!("not an array") };
            let numbers = |attribute: &Value<'_>| match attribute {
                Value::Value(inner) => match inner.as_ref() {
                    Value::Structure(attribute) => attribute.fields()[2..].iter().map(|field| u32::try_from(field).unwrap()).collect(),
                    other => panic!("not an attribute: {other:?}"),
                },
                other => panic!("not a variant: {other:?}"),
            };
            all.iter().map(numbers).collect()
        };
        assert_eq!(attributes(&text("committed", true)), [[UNDERLINE, SINGLE, 0, 9]]);
        // Three characters, whatever they take in bytes.
        assert_eq!(attributes(&text("三明治", true)), [[UNDERLINE, SINGLE, 0, 3]]);
        // Nothing to underline, and text that is the document's has no line.
        assert!(attributes(&text("", true)).is_empty());
        assert!(attributes(&text("hello", false)).is_empty());
    }

    #[test]
    fn an_engine_is_known_by_the_name_in_its_description() {
        assert_eq!(engine_name(&engine("libpinyin", "Intelligent Pinyin")).as_deref(), Some("libpinyin"));
        // As a property hands it over: a variant in a variant.
        assert_eq!(engine_name(&Value::Value(Box::new(engine("xkb:us::eng", "English")))).as_deref(), Some("xkb:us::eng"));
        assert_eq!(engine_name(&text("libpinyin", false)), None);
        assert_eq!(engine_name(&Value::from("libpinyin")), None);
    }
}
