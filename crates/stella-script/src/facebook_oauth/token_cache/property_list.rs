//! Property-list dates without SystemTime's platform-dependent lower bound.
//!
//! The plist crate handles object graphs, strings, dictionaries and validation.
//! Date objects temporarily use collision-free real markers while it parses or
//! writes. The final bytes retain the native 0x33 date tag and reference-epoch
//! f64, including SDK NSDate.distantPast (XML0000-12-30, before FILETIME1601).
use super::FacebookTokenCacheError;
use quick_xml::{
    Reader, Writer,
    events::{BytesEnd, BytesStart, BytesText, Event},
    name::QName,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
};

pub(super) type Dictionary = BTreeMap<String, Value>;
const REFERENCE_UNIX: f64 = 978_307_200.0;
type Result<T> = std::result::Result<T, FacebookTokenCacheError>;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Value {
    Array(Vec<Value>),
    Dictionary(Dictionary),
    /// Seconds since 2001-01-01, as stored by CFDate in binary plists.
    Date(f64),
    Other(plist::Value),
}

impl Value {
    pub fn date(unix: f64) -> Self {
        Self::Date(unix - REFERENCE_UNIX)
    }
    pub fn string(value: impl Into<String>) -> Self {
        Self::Other(plist::Value::String(value.into()))
    }
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::Other(value) => value.as_string(),
            _ => None,
        }
    }
    pub fn as_dictionary(&self) -> Option<&Dictionary> {
        match self {
            Self::Dictionary(value) => Some(value),
            _ => None,
        }
    }
    pub fn into_dictionary(self) -> Option<Dictionary> {
        match self {
            Self::Dictionary(value) => Some(value),
            _ => None,
        }
    }
    pub fn unix_date(&self) -> Option<f64> {
        match self {
            Self::Date(value) => Some(value + REFERENCE_UNIX),
            _ => None,
        }
    }
}

#[derive(Default)]
struct Markers {
    used: BTreeSet<u64>,
    dates: BTreeMap<u64, f64>,
}
impl Markers {
    fn insert(&mut self, date: f64) -> f64 {
        // These large finite f64 values cannot be emitted as 32-bit reals.
        let mut bits = f64::MAX.to_bits();
        while self.used.contains(&bits) {
            bits -= 1;
        }
        self.used.insert(bits);
        self.dates.insert(bits, date);
        f64::from_bits(bits)
    }
}

fn invalid() -> FacebookTokenCacheError {
    FacebookTokenCacheError::InvalidPreferences
}

/// Inspect only the offset table. The plist parser validates the object graph.
fn offsets(bytes: &[u8]) -> Result<Vec<usize>> {
    if bytes.len() < 40 || !bytes.starts_with(b"bplist00") {
        return Err(invalid());
    }
    let trailer = &bytes[bytes.len() - 32..];
    let width = usize::from(trailer[6]);
    if !(1..=8).contains(&width) {
        return Err(invalid());
    }
    let integer = |slice: &[u8]| -> Result<usize> {
        let mut value = [0_u8; 8];
        value[8 - slice.len()..].copy_from_slice(slice);
        usize::try_from(u64::from_be_bytes(value)).map_err(|_| invalid())
    };
    let count = integer(&trailer[8..16])?;
    let start = integer(&trailer[24..32])?;
    let length = count.checked_mul(width).ok_or_else(invalid)?;
    let end = start.checked_add(length).ok_or_else(invalid)?;
    if start < 8 || end > bytes.len() - 32 {
        return Err(invalid());
    }
    bytes[start..end]
        .chunks_exact(width)
        .map(|offset| {
            let offset = integer(offset)?;
            if offset < 8 || offset >= start {
                return Err(invalid());
            }
            if matches!(bytes[offset], 0x23 | 0x33)
                && offset.checked_add(9).is_none_or(|end| end > start)
            {
                return Err(invalid());
            }
            Ok(offset)
        })
        .collect()
}

fn real_bits(bytes: &[u8], offset: usize) -> u64 {
    u64::from_be_bytes(
        bytes[offset + 1..offset + 9]
            .try_into()
            .expect("checked date/real span"),
    )
}

pub(super) fn read(bytes: &[u8]) -> Result<Value> {
    let mut markers = Markers::default();
    let prepared = if bytes.starts_with(b"bplist00") {
        let offsets = offsets(bytes)?;
        for &offset in &offsets {
            if bytes[offset] == 0x23 {
                markers.used.insert(real_bits(bytes, offset));
            }
        }
        let mut prepared = bytes.to_vec();
        for offset in offsets {
            if bytes[offset] == 0x33 {
                let marker = markers.insert(f64::from_bits(real_bits(bytes, offset)));
                prepared[offset] = 0x23;
                prepared[offset + 1..offset + 9].copy_from_slice(&marker.to_be_bytes());
            }
        }
        prepared
    } else {
        prepare_xml(bytes, &mut markers)?
    };
    let value = plist::Value::from_reader(Cursor::new(prepared)).map_err(|_| invalid())?;
    from_plist(value, &markers)
}

fn from_plist(value: plist::Value, markers: &Markers) -> Result<Value> {
    Ok(match value {
        plist::Value::Dictionary(dict) => Value::Dictionary(
            dict.into_iter()
                .map(|(key, value)| Ok((key, from_plist(value, markers)?)))
                .collect::<Result<_>>()?,
        ),
        plist::Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|v| from_plist(v, markers))
                .collect::<Result<_>>()?,
        ),
        plist::Value::Real(value) if markers.dates.contains_key(&value.to_bits()) => {
            Value::Date(markers.dates[&value.to_bits()])
        }
        // Every date must have passed through the platform-independent path.
        plist::Value::Date(_) => return Err(invalid()),
        other => Value::Other(other),
    })
}

pub(super) fn write(value: &Value) -> Result<Vec<u8>> {
    let mut markers = Markers::default();
    collect_reals(value, &mut markers.used);
    let value = to_plist(value, &mut markers)?;
    let mut bytes = Vec::new();
    value.to_writer_binary(&mut bytes).map_err(|_| invalid())?;
    let mut written = BTreeSet::new();
    for offset in offsets(&bytes)? {
        if bytes[offset] == 0x23 {
            let bits = real_bits(&bytes, offset);
            if let Some(date) = markers.dates.get(&bits) {
                bytes[offset] = 0x33;
                bytes[offset + 1..offset + 9].copy_from_slice(&date.to_be_bytes());
                written.insert(bits);
            }
        }
    }
    if written.len() != markers.dates.len() {
        return Err(invalid());
    }
    Ok(bytes)
}

fn collect_reals(value: &Value, used: &mut BTreeSet<u64>) {
    match value {
        Value::Dictionary(dict) => {
            for v in dict.values() {
                collect_reals(v, used);
            }
        }
        Value::Array(array) => {
            for v in array {
                collect_reals(v, used);
            }
        }
        Value::Other(plist::Value::Real(real)) => {
            used.insert(real.to_bits());
        }
        _ => {}
    }
}

fn to_plist(value: &Value, markers: &mut Markers) -> Result<plist::Value> {
    Ok(match value {
        Value::Dictionary(dict) => plist::Value::Dictionary(
            dict.iter()
                .map(|(key, value)| Ok((key.clone(), to_plist(value, markers)?)))
                .collect::<Result<_>>()?,
        ),
        Value::Array(array) => plist::Value::Array(
            array
                .iter()
                .map(|v| to_plist(v, markers))
                .collect::<Result<_>>()?,
        ),
        Value::Date(date) => plist::Value::Real(markers.insert(*date)),
        Value::Other(
            plist::Value::Date(_) | plist::Value::Dictionary(_) | plist::Value::Array(_),
        ) => return Err(invalid()),
        Value::Other(value) => value.clone(),
    })
}

fn prepare_xml(bytes: &[u8], markers: &mut Markers) -> Result<Vec<u8>> {
    let mut reader = Reader::from_reader(bytes);
    loop {
        match reader.read_event().map_err(|_| invalid())? {
            Event::Start(tag) if tag.name() == QName("real") => {
                let text = reader.read_text(tag.name()).map_err(|_| invalid())?;
                let text = quick_xml::escape::unescape(&text).map_err(|_| invalid())?;
                let number: f64 = text.trim().parse().map_err(|_| invalid())?;
                markers.used.insert(number.to_bits());
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut reader = Reader::from_reader(bytes);
    let mut writer = Writer::new(Vec::new());
    loop {
        let event = reader.read_event().map_err(|_| invalid())?;
        match event {
            Event::Start(tag) if tag.name() == QName("date") => {
                let text = reader.read_text(tag.name()).map_err(|_| invalid())?;
                let text = quick_xml::escape::unescape(&text).map_err(|_| invalid())?;
                let date = time::OffsetDateTime::parse(
                    text.trim(),
                    &time::format_description::well_known::Rfc3339,
                )
                .map_err(|_| invalid())?;
                let reference = (date.unix_timestamp() as f64 - REFERENCE_UNIX)
                    + f64::from(date.nanosecond()) / 1e9;
                let marker = markers.insert(reference).to_string();
                writer
                    .write_event(Event::Start(BytesStart::new("real")))
                    .map_err(|_| invalid())?;
                writer
                    .write_event(Event::Text(BytesText::new(&marker)))
                    .map_err(|_| invalid())?;
                writer
                    .write_event(Event::End(BytesEnd::new("real")))
                    .map_err(|_| invalid())?;
            }
            Event::Eof => break,
            event => writer.write_event(event).map_err(|_| invalid())?,
        }
    }
    Ok(writer.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facebook_oauth_plist_dates_preserve_native_bytes_without_real_collisions() {
        let foundation = read(include_bytes!("fixtures/foundation-dates.plist")).unwrap();
        let foundation = foundation.as_dictionary().unwrap();
        assert_eq!(foundation["past"], Value::date(-62_135_769_600.0));
        assert_eq!(foundation["future"], Value::date(64_092_211_200.0));
        let value = Value::Array(vec![
            Value::date(-62_135_769_600.0),
            Value::date(64_092_211_200.0),
            Value::Date(0.125),
            Value::Other(plist::Value::Real(f64::MAX)),
            Value::Other(plist::Value::Real(f64::from_bits(f64::MAX.to_bits() - 1))),
            Value::Other(plist::Value::Data(vec![0x33, 0x23, 0, 0, 0, 0, 0, 0, 0])),
            Value::Dictionary(Dictionary::from([("nested".into(), Value::Date(-0.125))])),
        ]);
        let bytes = write(&value).unwrap();
        assert_eq!(read(&bytes).unwrap(), value);
        let native_dates: Vec<_> = offsets(&bytes)
            .unwrap()
            .into_iter()
            .filter(|&offset| bytes[offset] == 0x33)
            .map(|offset| f64::from_bits(real_bits(&bytes, offset)))
            .collect();
        assert_eq!(native_dates.len(), 4);
        assert!(native_dates.contains(&(-62_135_769_600.0 - REFERENCE_UNIX)));
        assert!(native_dates.contains(&(64_092_211_200.0 - REFERENCE_UNIX)));
        #[cfg(unix)]
        {
            // Independent unmodified plist Date decoding on platforms whose
            // SystemTime supports distantPast. The production codec never needs it.
            let native = plist::Value::from_reader(Cursor::new(&bytes)).unwrap();
            let array = native.as_array().unwrap();
            assert_eq!(
                array[0].as_date().unwrap().to_xml_format(),
                "0000-12-30T00:00:00Z"
            );
            assert_eq!(
                array[1].as_date().unwrap().to_xml_format(),
                "4001-01-01T00:00:00Z"
            );
        }
        for length in [0, 7, 8, 30, bytes.len() - 1] {
            assert!(read(&bytes[..length]).is_err());
        }
    }

    #[test]
    fn facebook_oauth_xml_dates_preserve_whitespace_and_native_epoch() {
        let xml = format!(
            r#"<?xml version="1.0"?><plist version="1.0"><dict>
            <key>past</key><date>0000-12-30T00:00:00Z</date>
            <key>future</key><date>4001-01-01T00:00:00Z</date>
            <key>fraction</key><date>2001-01-01T00:00:00.125Z</date>
            <key>token</key><string>  synthetic &amp; unchanged  </string>
            <key>real</key><real>{}</real>
        </dict></plist>"#,
            f64::MAX
        );
        let decoded = read(xml.as_bytes()).unwrap();
        let dict = decoded.as_dictionary().unwrap();
        assert_eq!(dict["past"], Value::date(-62_135_769_600.0));
        assert_eq!(dict["future"], Value::date(64_092_211_200.0));
        assert_eq!(dict["fraction"], Value::Date(0.125));
        assert_eq!(dict["token"].as_string(), Some("  synthetic & unchanged  "));
        assert_eq!(dict["real"], Value::Other(plist::Value::Real(f64::MAX)));
        assert_eq!(read(&write(&decoded).unwrap()).unwrap(), decoded);
    }
}
