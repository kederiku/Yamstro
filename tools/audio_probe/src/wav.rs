//! Lecture minimale d'un WAV PCM 16 bits mono. Le signal attendu ne passe pas par le décodeur
//! que le banc mesure : il est relu ici, octet par octet.

use std::path::Path;

pub fn read_mono_pcm16(path: &Path, expected_rate: u32) -> Vec<f32> {
    let bytes =
        std::fs::read(path).unwrap_or_else(|e| panic!("lecture de {} : {e}", path.display()));
    assert!(
        bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE",
        "pas un WAV"
    );
    let (mut at, mut format_ok, mut data) = (12, false, None);
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().expect("taille")) as usize;
        let body = &bytes[at + 8..at + 8 + size];
        if id == b"fmt " {
            let format = u16::from_le_bytes([body[0], body[1]]);
            let channels = u16::from_le_bytes([body[2], body[3]]);
            let rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
            let bits = u16::from_le_bytes([body[14], body[15]]);
            assert_eq!(
                (format, channels, rate, bits),
                (1, 1, expected_rate, 16),
                "format inattendu"
            );
            format_ok = true;
        } else if id == b"data" {
            data = Some(body);
        }
        at += 8 + size + (size & 1);
    }
    assert!(format_ok, "bloc fmt absent");
    data.expect("bloc data absent")
        .chunks_exact(2)
        .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
        .collect()
}
