//! Tiny shared XML helpers used by both the SOAP response parser and the
//! GENA `LastChange` parser.

/// Strips a namespace prefix off a `quick_xml` element/attribute name
/// (`u:PlayResponse` -> `PlayResponse`) - every parser in this crate reads
/// elements by their unprefixed local name, the same convention the
/// renderer's own request parser uses server-side.
pub(crate) fn local_name(name: &[u8]) -> &str {
    let name = std::str::from_utf8(name).unwrap_or("");
    match name.rfind(':') {
        Some(pos) => &name[pos + 1..],
        None => name,
    }
}
