use crate::config::CharsetKind;

pub fn get_charset(kind: &CharsetKind) -> Vec<char> {
    // Half-width katakana: U+FF66–U+FF9D
    let katakana: Vec<char> = (0xFF66u32..=0xFF9Du32)
        .filter_map(char::from_u32)
        .collect();

    let uppercase: Vec<char> = (b'A'..=b'Z').map(|b| b as char).collect();
    let lowercase: Vec<char> = (b'a'..=b'z').map(|b| b as char).collect();
    let digits: Vec<char> = (b'0'..=b'9').map(|b| b as char).collect();

    match kind {
        CharsetKind::Katakana => katakana,
        CharsetKind::Latin => {
            uppercase.into_iter().chain(lowercase).chain(digits).collect()
        }
        CharsetKind::Binary => vec!['0', '1'],
        CharsetKind::Mixed => katakana
            .into_iter()
            .chain(uppercase)
            .chain(digits)
            .collect(),
    }
}
