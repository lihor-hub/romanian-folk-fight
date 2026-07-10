//! Best-effort, dependency-cheap metadata probes for nonvisual media
//! (#197): a hand-rolled minimal Ogg/Vorbis container reader (duration,
//! sample rate, channel count -- every `audio/*.ogg` in this repo is
//! Vorbis-in-Ogg, produced by `scripts/generate-audio.py`) and a thin
//! `ttf-parser` wrapper for font family/metrics. Every probe degrades to
//! `None` on anything it cannot parse rather than failing the gallery run --
//! this is display metadata, not a validation rule (#185 already owns image
//! integrity; this module owns nothing pass/fail).

/// What could cheaply be read from an Ogg/Vorbis file's container headers.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct AudioProbe {
    pub duration_seconds: Option<f64>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
}

/// Parses just enough of the Ogg container to report duration/sample
/// rate/channel count for a Vorbis stream, without adding an audio-decoding
/// dependency to `xtask` (see `xtask/Cargo.toml`'s dependency-conservatism
/// note). Reads the first page's Vorbis identification packet for sample
/// rate/channels, and the last page carrying a valid granule position for
/// total sample count (duration = granule / sample_rate). This is a
/// container-header reader, not a general Ogg/Vorbis decoder: it assumes a
/// single logical bitstream (true for every file this repo generates) and
/// never validates CRCs or segment continuation across pages.
pub fn probe_ogg(bytes: &[u8]) -> AudioProbe {
    let mut probe = AudioProbe::default();
    let pages = find_ogg_pages(bytes);
    let Some(&first_page) = pages.first() else {
        return probe;
    };
    if let Some((channels, rate)) = parse_identification_header(bytes, first_page) {
        probe.channels = Some(channels);
        probe.sample_rate = Some(rate);
    }

    let mut last_granule: Option<u64> = None;
    for &offset in &pages {
        if let Some(granule) = granule_position(bytes, offset)
            && granule != u64::MAX
        {
            last_granule = Some(granule);
        }
    }
    if let (Some(granule), Some(rate)) = (last_granule, probe.sample_rate)
        && rate > 0
    {
        probe.duration_seconds = Some(granule as f64 / rate as f64);
    }
    probe
}

/// Byte offsets of every `"OggS"` page-sync marker in `bytes`, in order.
fn find_ogg_pages(bytes: &[u8]) -> Vec<usize> {
    let mut offsets = Vec::new();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if &bytes[i..i + 4] == b"OggS" {
            offsets.push(i);
            i += 4;
        } else {
            i += 1;
        }
    }
    offsets
}

/// Reads the 8-byte little-endian granule position at a page's fixed offset
/// (byte 6 of the Ogg page header), per the Ogg bitstream spec (RFC 3533).
fn granule_position(bytes: &[u8], page_offset: usize) -> Option<u64> {
    let start = page_offset + 6;
    let slice = bytes.get(start..start + 8)?;
    let arr: [u8; 8] = slice.try_into().ok()?;
    Some(u64::from_le_bytes(arr))
}

/// Parses the Vorbis identification packet expected in the first Ogg page's
/// payload: `page_segments` at header byte 26, the lacing/segment table
/// immediately after (26 fixed header bytes, indices 0..=25), then the
/// packet itself (`1` + `"vorbis"` + version(4) + channels(1) + sample
/// rate(4) + ...), per the Vorbis I spec section 4.2.2.
fn parse_identification_header(bytes: &[u8], page_offset: usize) -> Option<(u8, u32)> {
    let page_segments = *bytes.get(page_offset + 26)? as usize;
    let payload_start = page_offset + 27 + page_segments;
    let packet = bytes.get(payload_start..payload_start + 30)?;
    if packet[0] != 1 || &packet[1..7] != b"vorbis" {
        return None;
    }
    let channels = packet[11];
    let rate = u32::from_le_bytes(packet[12..16].try_into().ok()?);
    Some((channels, rate))
}

/// What could cheaply be read from a font's `name`/`head`/`maxp` tables.
#[derive(Debug, Default, Clone)]
pub struct FontProbe {
    pub family: Option<String>,
    pub units_per_em: Option<u16>,
    pub glyph_count: Option<u16>,
}

/// Uses `ttf-parser` (already an accepted dependency of the root game crate,
/// as a dev-dependency backing the diacritics-coverage test in
/// `src/core/mod.rs`) to read a font's family name and basic metrics without
/// decoding any glyph outlines.
pub fn probe_font(bytes: &[u8]) -> FontProbe {
    let mut probe = FontProbe::default();
    let Ok(face) = ttf_parser::Face::parse(bytes, 0) else {
        return probe;
    };
    probe.units_per_em = Some(face.units_per_em());
    probe.glyph_count = Some(face.number_of_glyphs());
    probe.family = face
        .names()
        .into_iter()
        .find(|name| name.name_id == ttf_parser::name_id::FAMILY && name.is_unicode())
        .and_then(|name| name.to_string());
    probe
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_non_ogg_file_probes_to_all_none_without_panicking() {
        let probe = probe_ogg(b"not an ogg file at all");
        assert_eq!(probe, AudioProbe::default());
    }

    #[test]
    fn an_empty_buffer_probes_cleanly() {
        assert_eq!(probe_ogg(&[]), AudioProbe::default());
    }

    #[test]
    fn a_real_bundled_ogg_file_probes_a_plausible_sample_rate_and_duration() {
        let bytes = include_bytes!("../../../../assets/audio/sfx_click.ogg");
        let probe = probe_ogg(bytes);
        let rate = probe
            .sample_rate
            .expect("sample rate parses from a real ogg file");
        assert!(
            (8_000..=192_000).contains(&rate),
            "plausible sample rate: {rate}"
        );
        let duration = probe
            .duration_seconds
            .expect("duration parses from a real ogg file");
        assert!(
            duration > 0.0 && duration < 60.0,
            "plausible short sfx duration: {duration}"
        );
    }

    #[test]
    fn a_non_font_buffer_probes_to_all_none_without_panicking() {
        let probe = probe_font(b"not a font");
        assert!(probe.family.is_none());
        assert!(probe.units_per_em.is_none());
    }

    #[test]
    fn the_bundled_font_probes_a_family_name_and_metrics() {
        let bytes = include_bytes!("../../../../assets/fonts/Alegreya-Variable.ttf");
        let probe = probe_font(bytes);
        assert!(probe.units_per_em.unwrap_or(0) > 0);
        assert!(probe.glyph_count.unwrap_or(0) > 0);
        assert!(
            probe
                .family
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains("alegreya"),
            "family name should mention Alegreya, got {:?}",
            probe.family
        );
    }
}
