use quick_xml::events::Event;
use quick_xml::reader::Reader;
use serde::Deserialize;

use crate::server::soap::error::{SoapError, SoapProtocolError};

pub fn extract_and_parse_soap_body<T>(xml: &str, action_name: &str) -> Result<T, SoapError>
where
    T: for<'de> Deserialize<'de>,
{
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut in_body = false;
    let mut action_content = String::new();

    loop {
        match reader.read_event()? {
            Event::Start(e) => {
                let name = e.name().into_inner();
                let name_str = std::str::from_utf8(name)?;

                if name_str.ends_with("Body") {
                    in_body = true;
                } else if in_body && name_str.ends_with(action_name) {
                    // Found our action, create a clean XML with just this action
                    action_content.push_str(&format!("<{}>", action_name));

                    // Parse all inner elements
                    let mut depth = 1;
                    while depth > 0 {
                        match reader.read_event()? {
                            Event::Start(inner_e) => {
                                depth += 1;
                                let tag = std::str::from_utf8(inner_e.name().into_inner())?;
                                // Remove namespace if present
                                let clean_tag = if let Some(pos) = tag.rfind(':') {
                                    &tag[pos + 1..]
                                } else {
                                    tag
                                };
                                action_content.push_str(&format!("<{}>", clean_tag));
                            }
                            Event::End(inner_e) => {
                                depth -= 1;
                                if depth > 0 {
                                    // Don't add the outer closing tag yet
                                    let tag = std::str::from_utf8(inner_e.name().into_inner())?;
                                    let clean_tag = if let Some(pos) = tag.rfind(':') {
                                        &tag[pos + 1..]
                                    } else {
                                        tag
                                    };
                                    action_content.push_str(&format!("</{}>\n", clean_tag));
                                }
                            }
                            Event::Text(t) => {
                                action_content.push_str(std::str::from_utf8(t.as_ref())?);
                            }
                            Event::Empty(empty_e) => {
                                let tag = std::str::from_utf8(empty_e.name().into_inner())?;
                                let clean_tag = if let Some(pos) = tag.rfind(':') {
                                    &tag[pos + 1..]
                                } else {
                                    tag
                                };
                                action_content.push_str(&format!("<{}/>", clean_tag));
                            }
                            Event::Eof => {
                                return Err(SoapError::Soap(SoapProtocolError::ParseError(
                                    format!("Action not Implemented Yet {}", action_name),
                                )))
                            }
                            _ => {}
                        }
                    }

                    // Add closing tag for our action
                    action_content.push_str(&format!("</{}>", action_name));
                    break;
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    if action_content.is_empty() {
        return Err(SoapError::Soap(SoapProtocolError::ParseError(format!(
            "Action Was not Provided {}",
            action_name
        ))));
    }

    // Parse the XML into the requested struct type
    let result: T = quick_xml::de::from_str(&action_content)?;

    Ok(result)
}
