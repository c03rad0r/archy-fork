//! Tag extraction for the music library (`13-MUSIC-MODEL.md`, D-13). Reads
//! title/artist/album/album-artist/track/disc/year/duration from an audio
//! file via `lofty`, confined to the node's configured media roots.
//!
//! `lofty` entered the tree through a `checkpoint:human-verify` package
//! legitimacy gate (13-04 Task 2) because 13-RESEARCH.md's Package
//! Legitimacy Audit marked it `[ASSUMED]` — the automated
//! `package-legitimacy check` seam was unavailable in the research session.

use lofty::prelude::*;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Raw tag data extracted from an audio file.
///
/// `has_tags` is `false` when the file is valid, readable audio with no tag
/// block — `title` is still populated (derived from the filename stem)
/// rather than left absent, because an untagged file must still appear in
/// the library.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RawTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub track: Option<u32>,
    pub disc: Option<u32>,
    pub year: Option<u32>,
    pub duration_secs: u64,
    pub has_tags: bool,
}

/// Errors `extract_tags` can return. Distinguished from the untagged case
/// (which is `Ok(RawTags { has_tags: false, .. })`, not an error) per the
/// 13-04 Task 3 behavior contract.
#[derive(Debug, Error)]
pub enum TagExtractionError {
    /// The indexer was pointed outside its configured media roots. Refused
    /// before any file content is read (T-13-20 — an indexer that can be
    /// pointed at `data_dir/secrets` is a secret-exfiltration primitive).
    #[error("path {0} is outside the configured media roots")]
    PathOutsideMediaRoots(PathBuf),
    /// The path could not be canonicalized (e.g. it doesn't exist).
    #[error("failed to resolve path: {0}")]
    Io(#[from] std::io::Error),
    /// `lofty` could not identify or parse the file as audio — distinct
    /// from the untagged case, which is `Ok`, not `Err` (T-13-21: a
    /// malformed file is a normal error path, never a panic).
    #[error("file is not readable audio: {0}")]
    NotAudio(#[from] lofty::error::LoftyError),
}

/// Extracts tags from the audio file at `path`.
///
/// `path` is canonicalized and confined to `media_roots` *before* the file
/// is opened. `media_roots` is a parameter, not a constant, so a caller
/// cannot bypass the confinement by construction.
pub fn extract_tags(path: &Path, media_roots: &[PathBuf]) -> Result<RawTags, TagExtractionError> {
    let canonical = path.canonicalize()?;

    let within_roots = media_roots.iter().any(|root| {
        root.canonicalize()
            .map(|canonical_root| canonical.starts_with(&canonical_root))
            .unwrap_or(false)
    });
    if !within_roots {
        return Err(TagExtractionError::PathOutsideMediaRoots(canonical));
    }

    let tagged_file = lofty::read_from_path(&canonical)?;
    let duration_secs = tagged_file.properties().duration().as_secs();

    let Some(tag) = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())
    else {
        return Ok(RawTags {
            title: Some(fallback_from_filename(&canonical)),
            duration_secs,
            has_tags: false,
            ..Default::default()
        });
    };

    let title = tag
        .title()
        .map(|cow| cow.into_owned())
        .unwrap_or_else(|| fallback_from_filename(&canonical));
    let artist = tag.artist().map(|cow| cow.into_owned());
    let album = tag.album().map(|cow| cow.into_owned());
    let album_artist = tag.get_string(ItemKey::AlbumArtist).map(ToOwned::to_owned);
    let track = tag.track();
    let disc = tag.disk();
    let year = tag.date().map(|timestamp| u32::from(timestamp.year));

    Ok(RawTags {
        title: Some(title),
        artist,
        album,
        album_artist,
        track,
        disc,
        year,
        duration_secs,
        has_tags: true,
    })
}

/// Derives a display title from a file's name when no tag block is present.
/// An untagged file still needs a title to display in the library.
fn fallback_from_filename(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn write_fixture(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).expect("write fixture file");
        path
    }

    // ---------------------------------------------------------------
    // Shared byte-level fixture builders. These construct minimal but
    // real containers programmatically (per Task 3's `<action>`: no
    // binary audio fixtures are committed — everything here is Rust
    // source producing bytes at test-run time into a `tempfile::tempdir`).
    // ---------------------------------------------------------------

    /// ID3v2.4 uses a "synchsafe" integer for tag/frame sizes: 4 bytes,
    /// 7 usable bits each, top bit always 0.
    fn synchsafe(mut n: u32) -> [u8; 4] {
        let mut out = [0u8; 4];
        for i in (0..4).rev() {
            out[i] = (n & 0x7F) as u8;
            n >>= 7;
        }
        out
    }

    fn id3v24_text_frame(id: &[u8; 4], text: &str) -> Vec<u8> {
        let mut content = Vec::new();
        content.push(3u8); // text encoding: UTF-8
        content.extend_from_slice(text.as_bytes());

        let mut frame = Vec::new();
        frame.extend_from_slice(id);
        frame.extend_from_slice(&synchsafe(content.len() as u32));
        frame.extend_from_slice(&[0, 0]); // frame flags
        frame.extend_from_slice(&content);
        frame
    }

    /// A real MPEG-1 Layer III frame sync (`FF FB 52 C4`): V1/Layer3,
    /// 64kbps, 44100Hz, mono, padded — the exact byte pattern lofty's own
    /// test suite uses as a known-good frame header
    /// (`mpeg/header.rs::tests::rev_search_for_frame_header`). Its computed
    /// frame length matches lofty's `Header::read` exactly:
    /// `samples * bitrate * 125 / sample_rate` truncates *before* the
    /// padding byte is added — `1152 * 64 * 125 / 44100 = 208`, `+ 1`
    /// padding = **209** bytes. The frames written below must be exactly
    /// this long, because lofty confirms a frame sync by comparing the
    /// candidate header against the bytes found exactly `len` later
    /// (`find_next_frame`/`cmp_header`); an off-by-one frame length makes
    /// the whole file "contain an invalid frame".
    const MP3_FRAME_HEADER: [u8; 4] = [0xFF, 0xFB, 0x52, 0xC4];
    const MP3_FRAME_LEN: usize = 209;

    fn mp3_frame() -> Vec<u8> {
        let mut frame = MP3_FRAME_HEADER.to_vec();
        frame.resize(MP3_FRAME_LEN, 0);
        frame
    }

    /// Builds a minimal MP3: an ID3v2.4 tag followed by two identical,
    /// correctly-sized frames. lofty's frame-sync search
    /// (`find_next_frame`/`cmp_header`) confirms a sync by comparing a
    /// candidate frame header against the one found exactly one frame
    /// length later, so two frames are required — a single frame is not
    /// enough to be accepted as a confirmed sync.
    fn build_mp3(text_frames: &[(&[u8; 4], &str)]) -> Vec<u8> {
        let mut frames = Vec::new();
        for (id, text) in text_frames {
            frames.extend(id3v24_text_frame(id, text));
        }

        let mut file = Vec::new();
        file.extend_from_slice(b"ID3");
        file.push(4); // major version
        file.push(0); // minor version
        file.push(0); // flags
        file.extend_from_slice(&synchsafe(frames.len() as u32));
        file.extend_from_slice(&frames);
        file.extend_from_slice(&mp3_frame());
        file.extend_from_slice(&mp3_frame());
        file
    }

    /// FLAC STREAMINFO block content (34 bytes) — the only metadata block
    /// lofty's duration calculation reads (`flac/properties.rs`).
    fn flac_streaminfo(
        sample_rate: u32,
        channels: u32,
        bits_per_sample: u32,
        total_samples: u64,
    ) -> Vec<u8> {
        let mut info: u32 =
            (sample_rate << 12) | ((channels - 1) << 9) | ((bits_per_sample - 1) << 4);
        info |= ((total_samples >> 32) as u32) & 0xF;
        let total_samples_low = (total_samples & 0xFFFF_FFFF) as u32;

        let mut content = Vec::with_capacity(34);
        content.extend_from_slice(&[0, 0, 0, 0]); // min/max block size (unused)
        content.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // min/max frame size (unused)
        content.extend_from_slice(&info.to_be_bytes());
        content.extend_from_slice(&total_samples_low.to_be_bytes());
        content.extend_from_slice(&[0u8; 16]); // MD5 signature (unused)
        content
    }

    fn flac_block(block_type: u8, is_last: bool, content: &[u8]) -> Vec<u8> {
        let mut block = Vec::with_capacity(4 + content.len());
        let header_byte = ((is_last as u8) << 7) | (block_type & 0x7F);
        let len = content.len() as u32;
        block.push(header_byte);
        block.push(((len >> 16) & 0xFF) as u8);
        block.push(((len >> 8) & 0xFF) as u8);
        block.push((len & 0xFF) as u8);
        block.extend_from_slice(content);
        block
    }

    /// The standard Vorbis comment payload (vendor string + KEY=VALUE
    /// list, little-endian length prefixes) — shared by the FLAC
    /// VORBIS_COMMENT metadata block and the OGG Vorbis comment packet,
    /// which use the identical format.
    fn vorbis_comment_block(vendor: &str, comments: &[(&str, &str)]) -> Vec<u8> {
        let mut content = Vec::new();
        content.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        content.extend_from_slice(vendor.as_bytes());
        content.extend_from_slice(&(comments.len() as u32).to_le_bytes());
        for (key, value) in comments {
            let field = format!("{key}={value}");
            content.extend_from_slice(&(field.len() as u32).to_le_bytes());
            content.extend_from_slice(field.as_bytes());
        }
        content
    }

    const FLAC_SAMPLE_RATE: u32 = 44100;
    const FLAC_DURATION_SECS: u64 = 3;

    fn build_flac(comments: Option<&[(&str, &str)]>) -> Vec<u8> {
        let streaminfo = flac_streaminfo(
            FLAC_SAMPLE_RATE,
            2,
            16,
            u64::from(FLAC_SAMPLE_RATE) * FLAC_DURATION_SECS,
        );

        let mut file = Vec::new();
        file.extend_from_slice(b"fLaC");
        match comments {
            None => file.extend_from_slice(&flac_block(0, true, &streaminfo)),
            Some(comments) => {
                file.extend_from_slice(&flac_block(0, false, &streaminfo));
                let vorbis_comments = vorbis_comment_block("test-vendor", comments);
                file.extend_from_slice(&flac_block(4, true, &vorbis_comments));
            }
        }
        file
    }

    /// Generic ISO-BMFF (MP4) atom: 4-byte big-endian length (including
    /// this header) + 4-byte fourcc + content.
    fn atom(fourcc: &[u8; 4], content: &[u8]) -> Vec<u8> {
        let len = (8 + content.len()) as u32;
        let mut out = Vec::with_capacity(len as usize);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(fourcc);
        out.extend_from_slice(content);
        out
    }

    /// An MP4 metadata "data" atom: type-set byte (0 = well-known) + 3-byte
    /// type code + 4-byte locale (unused) + content.
    fn mp4_data_atom(type_code: u32, content: &[u8]) -> Vec<u8> {
        let mut data_content = Vec::new();
        data_content.push(0u8);
        data_content.extend_from_slice(&type_code.to_be_bytes()[1..]);
        data_content.extend_from_slice(&[0, 0, 0, 0]);
        data_content.extend_from_slice(content);
        atom(b"data", &data_content)
    }

    /// A UTF-8 text ilst item (DataType::Utf8 = 1), e.g. `\xa9nam`/title.
    fn mp4_text_item(fourcc: &[u8; 4], text: &str) -> Vec<u8> {
        let data = mp4_data_atom(1, text.as_bytes());
        atom(fourcc, &data)
    }

    /// A `trkn`/`disk`-shaped current/total pair item (DataType::Reserved
    /// = 0, 8-byte binary payload: `[0,0,cur_hi,cur_lo,tot_hi,tot_lo,0,0]`).
    fn mp4_int_pair_item(fourcc: &[u8; 4], current: u16, total: u16) -> Vec<u8> {
        let mut content = vec![0u8, 0u8];
        content.extend_from_slice(&current.to_be_bytes());
        content.extend_from_slice(&total.to_be_bytes());
        content.extend_from_slice(&[0u8, 0u8]);
        let data = mp4_data_atom(0, &content);
        atom(fourcc, &data)
    }

    const MP4_TIMESCALE: u32 = 44100;
    const MP4_DURATION_SECS: u64 = 3;

    /// Builds a minimal M4A: `ftyp` + `moov` containing a single audio
    /// `trak` (just `mdia.hdlr` + `mdia.mdhd`, no `minf`/`stbl` — lofty's
    /// property reader only needs `mdhd` for duration) and, when `ilst` is
    /// provided, a `moov.udta.meta.ilst` tag block.
    fn build_m4a(ilst: Option<&[u8]>) -> Vec<u8> {
        let mut ftyp_content = Vec::new();
        ftyp_content.extend_from_slice(b"M4A ");
        ftyp_content.extend_from_slice(&[0, 0, 0, 0]);
        ftyp_content.extend_from_slice(b"M4A ");
        let ftyp = atom(b"ftyp", &ftyp_content);

        let mut hdlr_content = Vec::new();
        hdlr_content.extend_from_slice(&[0, 0, 0, 0]); // version + flags
        hdlr_content.extend_from_slice(&[0, 0, 0, 0]); // pre_defined
        hdlr_content.extend_from_slice(b"soun"); // handler type: audio
        hdlr_content.extend_from_slice(&[0, 0, 0, 0]); // reserved
        let hdlr = atom(b"hdlr", &hdlr_content);

        let mut mdhd_content = Vec::new();
        mdhd_content.push(0); // version 0
        mdhd_content.extend_from_slice(&[0, 0, 0]); // flags
        mdhd_content.extend_from_slice(&[0, 0, 0, 0]); // creation_time
        mdhd_content.extend_from_slice(&[0, 0, 0, 0]); // modification_time
        mdhd_content.extend_from_slice(&MP4_TIMESCALE.to_be_bytes());
        mdhd_content
            .extend_from_slice(&((MP4_TIMESCALE as u64 * MP4_DURATION_SECS) as u32).to_be_bytes());
        let mdhd = atom(b"mdhd", &mdhd_content);

        let mdia = atom(b"mdia", &[hdlr, mdhd].concat());
        let trak = atom(b"trak", &mdia);

        let mut moov_content = trak;
        if let Some(ilst_bytes) = ilst {
            let ilst_atom = atom(b"ilst", ilst_bytes);
            let mut meta_content = Vec::new();
            meta_content.extend_from_slice(&[0, 0, 0, 0]); // full-meta version + flags
            meta_content.extend_from_slice(&ilst_atom);
            let meta = atom(b"meta", &meta_content);
            let udta = atom(b"udta", &meta);
            moov_content.extend_from_slice(&udta);
        }
        let moov = atom(b"moov", &moov_content);

        let mut file = Vec::new();
        file.extend_from_slice(&ftyp);
        file.extend_from_slice(&moov);
        file
    }

    /// A single OGG page: header + lacing segment table + packet content.
    /// Every packet used by these fixtures is well under 255 bytes, so
    /// each is exactly one lacing segment — no multi-segment continuation
    /// handling is needed. The checksum field is written as 0; `ogg_pager`
    /// (lofty's OGG page reader) never validates it.
    fn ogg_page(header_type: u8, abgp: u64, serial: u32, seq: u32, packets: &[&[u8]]) -> Vec<u8> {
        let mut segment_table = Vec::new();
        let mut content = Vec::new();
        for packet in packets {
            assert!(
                packet.len() < 255,
                "fixture packet too large for a single OGG lacing segment"
            );
            segment_table.push(packet.len() as u8);
            content.extend_from_slice(packet);
        }

        let mut page = Vec::new();
        page.extend_from_slice(b"OggS");
        page.push(0); // version
        page.push(header_type);
        page.extend_from_slice(&abgp.to_le_bytes());
        page.extend_from_slice(&serial.to_le_bytes());
        page.extend_from_slice(&seq.to_le_bytes());
        page.extend_from_slice(&0u32.to_le_bytes()); // checksum, unchecked on read
        page.push(segment_table.len() as u8);
        page.extend_from_slice(&segment_table);
        page.extend_from_slice(&content);
        page
    }

    fn vorbis_ident_packet(sample_rate: u32, channels: u8) -> Vec<u8> {
        let mut packet = vec![1u8, b'v', b'o', b'r', b'b', b'i', b's'];
        packet.extend_from_slice(&1u32.to_le_bytes()); // vorbis_version
        packet.push(channels);
        packet.extend_from_slice(&sample_rate.to_le_bytes());
        packet.extend_from_slice(&0i32.to_le_bytes()); // bitrate_maximum
        packet.extend_from_slice(&128_000i32.to_le_bytes()); // bitrate_nominal
        packet.extend_from_slice(&0i32.to_le_bytes()); // bitrate_minimum
        packet
    }

    fn vorbis_comment_packet(vendor: &str, comments: &[(&str, &str)]) -> Vec<u8> {
        let mut packet = vec![3u8, b'v', b'o', b'r', b'b', b'i', b's'];
        packet.extend_from_slice(&vorbis_comment_block(vendor, comments));
        packet
    }

    const OGG_SAMPLE_RATE: u32 = 44100;
    const OGG_DURATION_SECS: u64 = 3;

    /// Builds a minimal OGG Vorbis stream: one header page carrying the
    /// three mandatory header packets (identification, comment, setup —
    /// lofty's page reader requires exactly 3 to accept the sync) and one
    /// trailing "audio" page whose absolute granule position encodes the
    /// total sample count lofty computes duration from.
    fn build_ogg(comments: &[(&str, &str)]) -> Vec<u8> {
        let ident = vorbis_ident_packet(OGG_SAMPLE_RATE, 2);
        let comment = vorbis_comment_packet("test-vendor", comments);
        let setup = vec![5u8]; // setup packet content is never parsed here

        let header_page = ogg_page(0x02, 0, 1, 0, &[&ident, &comment, &setup]);

        let total_samples = u64::from(OGG_SAMPLE_RATE) * OGG_DURATION_SECS;
        let audio_page = ogg_page(0x04, total_samples, 1, 1, &[&[0u8]]);

        let mut file = Vec::new();
        file.extend_from_slice(&header_page);
        file.extend_from_slice(&audio_page);
        file
    }

    // ---------------------------------------------------------------
    // Tests
    // ---------------------------------------------------------------

    #[test]
    fn mp3_id3v24_yields_full_record() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = build_mp3(&[
            (b"TIT2", "Test Title"),
            (b"TPE1", "Test Artist"),
            (b"TALB", "Test Album"),
            (b"TPE2", "Test Album Artist"),
            (b"TRCK", "5"),
            (b"TPOS", "1"),
            (b"TDRC", "2024"),
        ]);
        let path = write_fixture(dir.path(), "full.mp3", &bytes);
        let roots = vec![dir.path().to_path_buf()];

        let tags = extract_tags(&path, &roots).expect("mp3 with real tags should extract");

        assert_eq!(tags.title.as_deref(), Some("Test Title"));
        assert_eq!(tags.artist.as_deref(), Some("Test Artist"));
        assert_eq!(tags.album.as_deref(), Some("Test Album"));
        assert_eq!(tags.album_artist.as_deref(), Some("Test Album Artist"));
        assert_eq!(tags.track, Some(5));
        assert_eq!(tags.disc, Some(1));
        assert_eq!(tags.year, Some(2024));
        assert!(tags.has_tags);
    }

    #[test]
    fn flac_vorbis_yields_full_record() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = build_flac(Some(&[
            ("TITLE", "Test Title"),
            ("ARTIST", "Test Artist"),
            ("ALBUM", "Test Album"),
            ("ALBUMARTIST", "Test Album Artist"),
            ("TRACKNUMBER", "5"),
            ("DISCNUMBER", "1"),
            ("DATE", "2024"),
        ]));
        let path = write_fixture(dir.path(), "full.flac", &bytes);
        let roots = vec![dir.path().to_path_buf()];

        let tags = extract_tags(&path, &roots).expect("flac with real tags should extract");

        assert_eq!(tags.title.as_deref(), Some("Test Title"));
        assert_eq!(tags.artist.as_deref(), Some("Test Artist"));
        assert_eq!(tags.album.as_deref(), Some("Test Album"));
        assert_eq!(tags.album_artist.as_deref(), Some("Test Album Artist"));
        assert_eq!(tags.track, Some(5));
        assert_eq!(tags.disc, Some(1));
        assert_eq!(tags.year, Some(2024));
        assert_eq!(tags.duration_secs, FLAC_DURATION_SECS);
        assert!(tags.has_tags);
    }

    #[test]
    fn m4a_yields_full_record() {
        let dir = tempfile::tempdir().unwrap();
        let ilst_bytes: Vec<u8> = [
            mp4_text_item(b"\xa9nam", "Test Title"),
            mp4_text_item(b"\xa9ART", "Test Artist"),
            mp4_text_item(b"\xa9alb", "Test Album"),
            mp4_text_item(b"aART", "Test Album Artist"),
            mp4_int_pair_item(b"trkn", 5, 0),
            mp4_int_pair_item(b"disk", 1, 0),
            mp4_text_item(b"\xa9day", "2024"),
        ]
        .concat();
        let bytes = build_m4a(Some(&ilst_bytes));
        let path = write_fixture(dir.path(), "full.m4a", &bytes);
        let roots = vec![dir.path().to_path_buf()];

        let tags = extract_tags(&path, &roots).expect("m4a with real tags should extract");

        assert_eq!(tags.title.as_deref(), Some("Test Title"));
        assert_eq!(tags.artist.as_deref(), Some("Test Artist"));
        assert_eq!(tags.album.as_deref(), Some("Test Album"));
        assert_eq!(tags.album_artist.as_deref(), Some("Test Album Artist"));
        assert_eq!(tags.track, Some(5));
        assert_eq!(tags.disc, Some(1));
        assert_eq!(tags.year, Some(2024));
        assert_eq!(tags.duration_secs, MP4_DURATION_SECS);
        assert!(tags.has_tags);
    }

    #[test]
    fn ogg_yields_full_record() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = build_ogg(&[
            ("TITLE", "Test Title"),
            ("ARTIST", "Test Artist"),
            ("ALBUM", "Test Album"),
            ("ALBUMARTIST", "Test Album Artist"),
            ("TRACKNUMBER", "5"),
            ("DISCNUMBER", "1"),
            ("DATE", "2024"),
        ]);
        let path = write_fixture(dir.path(), "full.ogg", &bytes);
        let roots = vec![dir.path().to_path_buf()];

        let tags = extract_tags(&path, &roots).expect("ogg with real tags should extract");

        assert_eq!(tags.title.as_deref(), Some("Test Title"));
        assert_eq!(tags.artist.as_deref(), Some("Test Artist"));
        assert_eq!(tags.album.as_deref(), Some("Test Album"));
        assert_eq!(tags.album_artist.as_deref(), Some("Test Album Artist"));
        assert_eq!(tags.track, Some(5));
        assert_eq!(tags.disc, Some(1));
        assert_eq!(tags.year, Some(2024));
        assert_eq!(tags.duration_secs, OGG_DURATION_SECS);
        assert!(tags.has_tags);
    }

    #[test]
    fn untagged_file_falls_back_to_filename_stem() {
        let dir = tempfile::tempdir().unwrap();
        // A valid, readable FLAC with no VORBIS_COMMENT block at all —
        // exercises the "readable audio, no tags" path distinctly from
        // the "not audio" path below.
        let bytes = build_flac(None);
        let path = write_fixture(dir.path(), "no_tags_here.flac", &bytes);
        let roots = vec![dir.path().to_path_buf()];

        let tags = extract_tags(&path, &roots).expect("untagged audio should not error");

        assert_eq!(tags.title.as_deref(), Some("no_tags_here"));
        assert_eq!(tags.artist, None);
        assert_eq!(tags.album, None);
        assert_eq!(tags.duration_secs, FLAC_DURATION_SECS);
        assert!(!tags.has_tags);
    }

    #[test]
    fn non_audio_returns_err_distinct_from_untagged() {
        let dir = tempfile::tempdir().unwrap();
        // Plain ASCII text (no 0xFF byte anywhere, so it can never
        // accidentally contain an MPEG frame sync) renamed to `.mp3`.
        let bytes = b"this is not audio at all, just plain text content for a test fixture";
        let path = write_fixture(dir.path(), "fake.mp3", bytes);
        let roots = vec![dir.path().to_path_buf()];

        let result = extract_tags(&path, &roots);

        assert!(
            matches!(result, Err(TagExtractionError::NotAudio(_))),
            "expected NotAudio, got {result:?}"
        );
    }

    #[test]
    fn path_outside_media_roots_is_refused() {
        let media_dir = tempfile::tempdir().unwrap();
        let outside_dir = tempfile::tempdir().unwrap();
        // Content is irrelevant — the root check must reject this before
        // any attempt to open/parse the file as audio.
        let path = write_fixture(outside_dir.path(), "secret.mp3", b"irrelevant");
        let roots = vec![media_dir.path().to_path_buf()];

        let result = extract_tags(&path, &roots);

        assert!(
            matches!(result, Err(TagExtractionError::PathOutsideMediaRoots(_))),
            "expected PathOutsideMediaRoots, got {result:?}"
        );
    }
}
