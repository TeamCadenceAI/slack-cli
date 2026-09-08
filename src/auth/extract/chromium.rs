//! Chromium LevelStore (LevelDB) extraction of Slack `xoxc-` tokens.
//!
//! Chromium-based browsers (and the Slack desktop app) persist DOM Local
//! Storage in a LevelDB directory (`Local Storage/leveldb/`) made up of
//! `*.ldb` sorted-table files and `*.log` write-ahead logs. Slack stores a
//! per-team blob under the Local Storage key `localConfig_v2`, whose JSON value
//! looks like:
//!
//! ```json
//! { "teams": { "T0123ABCD": { "name": "Acme", "domain": "acme", "token": "xoxc-…" } } }
//! ```
//!
//! This module recovers those `xoxc-` tokens without pulling in a full LevelDB
//! implementation. `*.ldb` files are minimally parsed as SSTables so that
//! Snappy-compressed data blocks can be decompressed; everything else is
//! scanned as raw bytes. Because LevelDB framing (and Chromium's UTF-16 value
//! encoding) routinely splits or mangles values, a strict JSON parse is
//! attempted first and a resilient hand-rolled scan is used as a fallback.
//!
//! The parsing here is deliberately OS-agnostic — it only touches bytes on
//! disk, so no `#[cfg(target_os = …)]` gating is required.

use crate::error::Result;
use std::path::{Path, PathBuf};

/// A single Slack team's client token recovered from a browser's LevelDB.
///
/// `team_id`, `domain`, and `name` are best-effort: LevelDB framing can split
/// values across blocks, so the surrounding metadata is not always recoverable
/// even when the `xoxc` token itself is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamToken {
    /// Slack team id (e.g. `T0123ABCD`), when it could be recovered.
    pub team_id: Option<String>,
    /// Workspace subdomain (e.g. `acme` for `acme.slack.com`), when recovered.
    pub domain: Option<String>,
    /// Human-readable workspace name, when recovered.
    pub name: Option<String>,
    /// The `xoxc-…` client token.
    pub xoxc: String,
}

/// The LevelDB SSTable footer magic (`kTableMagicNumber`), little-endian.
const SSTABLE_MAGIC: [u8; 8] = [0x57, 0xfb, 0x80, 0x8b, 0x24, 0x75, 0x47, 0xdb];

/// Extract every Slack `xoxc-` token found in a Chromium Local Storage
/// `leveldb` directory.
///
/// Reads all `*.ldb` and `*.log` files in `leveldb_dir`, recovers their bytes
/// (Snappy-decompressing SSTable data blocks best-effort), then parses the
/// `localConfig_v2` team blob. Results are de-duplicated by `xoxc` token, with
/// metadata fields merged across sources so the most complete record wins.
///
/// A missing or unreadable directory yields an empty vector rather than an
/// error — callers treat "no tokens here" and "no such browser profile" alike.
pub fn extract_tokens_from_leveldb(leveldb_dir: &Path) -> Result<Vec<TeamToken>> {
    let mut recovered: Vec<u8> = Vec::new();

    let entries = match std::fs::read_dir(leveldb_dir) {
        Ok(entries) => entries,
        // A profile without a Local Storage dir simply contributes nothing.
        Err(_) => return Ok(Vec::new()),
    };

    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    // Process newest files first so the most recent write of a value wins.
    // LevelDB keeps superseded values in older SSTables until compaction, so a
    // renamed workspace (e.g. "Cadence" -> "antiburn") can appear twice; the
    // freshest write is authoritative. We order by last-modified time
    // (descending), falling back to the filename so ordering stays
    // deterministic when timestamps tie or are unavailable.
    files.sort_by(|a, b| {
        let mtime = |p: &PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        mtime(b)
            .cmp(&mtime(a))
            .then_with(|| b.file_name().cmp(&a.file_name()))
    });

    for path in files {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let is_ldb = ext.eq_ignore_ascii_case("ldb");
        let is_log = ext.eq_ignore_ascii_case("log");
        if !is_ldb && !is_log {
            continue;
        }

        let data = match std::fs::read(&path) {
            Ok(data) => data,
            Err(_) => continue,
        };

        if is_ldb {
            // Preferred path: walk the SSTable and decompress its data blocks.
            if let Some(blocks) = sstable_data_bytes(&data) {
                recovered.extend_from_slice(&blocks);
                recovered.push(b'\n');
            }
            // Best-effort: some tools store a whole file as one Snappy frame.
            if let Ok(dec) = snap::raw::Decoder::new().decompress_vec(&data) {
                recovered.extend_from_slice(&dec);
                recovered.push(b'\n');
            }
        }

        // Ultimate fallback: raw bytes. `*.log` values are frequently
        // uncompressed, and even in `*.ldb` files this catches anything the
        // structured parse missed. The downstream scanner tolerates the noise.
        recovered.extend_from_slice(&data);
        recovered.push(b'\n');
    }

    Ok(parse_recovered(&recovered))
}

// ---------------------------------------------------------------------------
// Minimal LevelDB SSTable reader
// ---------------------------------------------------------------------------

/// Read an unsigned LEB128 varint, advancing `pos`. Returns `None` on overflow
/// or truncation.
fn read_varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    loop {
        let byte = *buf.get(*pos)?;
        *pos += 1;
        result |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(result);
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
}

/// Read a LevelDB `BlockHandle` (offset varint, size varint), advancing `pos`.
fn read_block_handle(buf: &[u8], pos: &mut usize) -> Option<(usize, usize)> {
    let offset = read_varint(buf, pos)? as usize;
    let size = read_varint(buf, pos)? as usize;
    Some((offset, size))
}

/// Read the contents of the block at `offset`/`size` within `file`,
/// transparently Snappy-decompressing when the trailing compression-type byte
/// says so. Returns `None` if the handle is out of bounds.
fn read_block(file: &[u8], offset: usize, size: usize) -> Option<Vec<u8>> {
    let end = offset.checked_add(size)?;
    // The compression-type byte lives immediately after the block contents.
    if end >= file.len() {
        return None;
    }
    let raw = &file[offset..end];
    match file[end] {
        0 => Some(raw.to_vec()),                                 // no compression
        1 => snap::raw::Decoder::new().decompress_vec(raw).ok(), // Snappy
        _ => Some(raw.to_vec()), // unknown (e.g. zstd): fall back to raw bytes
    }
}

/// Parse the entries of a LevelDB index block, returning the `BlockHandle`
/// (offset, size) encoded in each entry's value — i.e. the set of data blocks.
fn parse_index_handles(block: &[u8]) -> Vec<(usize, usize)> {
    let mut handles = Vec::new();
    let n = block.len();
    if n < 4 {
        return handles;
    }

    // The block trailer is `num_restarts` (u32 LE) preceded by that many u32
    // restart offsets. Entries occupy everything before the restart array.
    let num_restarts = u32::from_le_bytes([block[n - 4], block[n - 3], block[n - 2], block[n - 1]]);
    let trailer = match (num_restarts as usize)
        .checked_add(1)
        .and_then(|c| c.checked_mul(4))
    {
        Some(t) if t <= n => t,
        _ => return handles,
    };
    let end = n - trailer;

    let mut pos = 0usize;
    while pos < end {
        // Entry header: shared_key_len, non_shared_key_len, value_len.
        let _shared = match read_varint(block, &mut pos) {
            Some(v) => v,
            None => break,
        };
        let non_shared = match read_varint(block, &mut pos) {
            Some(v) => v as usize,
            None => break,
        };
        let value_len = match read_varint(block, &mut pos) {
            Some(v) => v as usize,
            None => break,
        };

        // Skip the key delta bytes.
        pos = match pos.checked_add(non_shared) {
            Some(p) if p <= end => p,
            _ => break,
        };

        let value_start = pos;
        pos = match pos.checked_add(value_len) {
            Some(p) if p <= end => p,
            _ => break,
        };

        // The index entry's value is the data block's handle.
        let mut hp = value_start;
        if let Some(handle) = read_block_handle(&block[..pos], &mut hp) {
            handles.push(handle);
        }
    }

    handles
}

/// Best-effort decode of a `*.ldb` SSTable into the concatenated (and
/// decompressed) contents of its data blocks. Returns `None` when the file is
/// not a recognizable SSTable, letting the caller fall back to a raw scan.
fn sstable_data_bytes(file: &[u8]) -> Option<Vec<u8>> {
    let n = file.len();
    // Footer is a fixed 48 bytes: two BlockHandles + padding + 8-byte magic.
    if n < 48 || file[n - 8..] != SSTABLE_MAGIC {
        return None;
    }

    let mut pos = n - 48;
    let _metaindex = read_block_handle(file, &mut pos)?; // unused
    let (index_off, index_size) = read_block_handle(file, &mut pos)?;

    let index_block = read_block(file, index_off, index_size)?;
    let handles = parse_index_handles(&index_block);

    let mut out = Vec::new();
    for (off, size) in handles {
        // A single bad handle shouldn't abort the whole file.
        if let Some(block) = read_block(file, off, size) {
            out.extend_from_slice(&block);
            out.push(b'\n');
        }
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Token parsing
// ---------------------------------------------------------------------------

/// Parse recovered bytes across multiple textual "views" and merge the results.
///
/// Chromium encodes non-ASCII Local Storage values as UTF-16LE, which litters
/// ASCII JSON with `\0` bytes and defeats naive substring search. We therefore
/// scan both the raw lossy-UTF-8 view and a null-stripped view.
fn parse_recovered(bytes: &[u8]) -> Vec<TeamToken> {
    let mut acc: Vec<TeamToken> = Vec::new();

    // View 1: raw bytes as lossy UTF-8 (covers plain-ASCII / Latin-1 values).
    let raw = String::from_utf8_lossy(bytes);
    collect_into(&mut acc, &raw);

    // View 2: null-stripped (collapses UTF-16LE ASCII down to plain ASCII).
    if bytes.contains(&0) {
        let stripped: Vec<u8> = bytes.iter().copied().filter(|&b| b != 0).collect();
        let s = String::from_utf8_lossy(&stripped);
        collect_into(&mut acc, &s);
    }

    acc
}

/// Run both the strict JSON parse and the resilient scan over one text view.
fn collect_into(acc: &mut Vec<TeamToken>, text: &str) {
    for tok in parse_local_config(text) {
        merge_token(acc, tok);
    }
    for tok in scan_teams(text) {
        merge_token(acc, tok);
    }
}

/// De-duplicate by `xoxc` token, filling in any metadata a later, more complete
/// record provides.
fn merge_token(acc: &mut Vec<TeamToken>, tok: TeamToken) {
    if let Some(existing) = acc.iter_mut().find(|t| t.xoxc == tok.xoxc) {
        if existing.team_id.is_none() {
            existing.team_id = tok.team_id;
        }
        if existing.domain.is_none() {
            existing.domain = tok.domain;
        }
        if existing.name.is_none() {
            existing.name = tok.name;
        }
    } else {
        acc.push(tok);
    }
}

/// Strict path: locate each `localConfig_v2` occurrence, extract the balanced
/// JSON object that follows, and read teams via `serde_json`.
fn parse_local_config(text: &str) -> Vec<TeamToken> {
    let mut out = Vec::new();

    for (idx, _) in text.match_indices("localConfig_v2") {
        let Some(obj) = extract_json_object(text, idx) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(obj) else {
            continue;
        };

        // Accept either `{ "teams": { … } }` or a bare team map.
        let teams = value
            .get("teams")
            .and_then(|t| t.as_object())
            .or_else(|| value.as_object());
        let Some(teams) = teams else {
            continue;
        };

        for (key, team) in teams {
            let token = team.get("token").and_then(|t| t.as_str());
            let Some(token) = token else { continue };
            if !token.starts_with("xoxc-") {
                continue;
            }
            let team_id = team
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .or_else(|| key.starts_with('T').then(|| key.clone()));
            out.push(TeamToken {
                team_id,
                domain: team
                    .get("domain")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                name: team
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                xoxc: token.to_string(),
            });
        }
    }

    out
}

/// Extract the first balanced `{…}` object at or after `from`, honoring JSON
/// string quoting so braces inside strings don't confuse the depth counter.
fn extract_json_object(text: &str, from: usize) -> Option<&str> {
    let bytes = text.as_bytes();
    let start = from + bytes[from..].iter().position(|&b| b == b'{')?;

    let mut depth = 0usize;
    let mut in_str = false;
    let mut escaped = false;

    let mut i = start;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_str = false;
            }
        } else {
            match c {
                b'"' => in_str = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        // `get` returns None (rather than panicking) if `i`
                        // lands mid-UTF-8 char, which is fine — we just skip.
                        return text.get(start..=i);
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

/// Resilient fallback: find every `xoxc-` token in `text` and recover as much
/// surrounding team metadata as possible. Works even when the value is a
/// backslash-escaped JSON string (as stored in `localStorage`) or when strict
/// parsing failed because LevelDB split the object across blocks.
fn scan_teams(text: &str) -> Vec<TeamToken> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();

    for (pos, _) in text.match_indices("xoxc-") {
        let xoxc = read_token(bytes, pos);
        if xoxc.len() < 12 {
            continue; // too short to be a real token; likely noise
        }

        let (obj_start, obj_end) = enclosing_object(bytes, pos);
        let object = text.get(obj_start..=obj_end).unwrap_or("");

        out.push(TeamToken {
            team_id: find_team_id_before(text, obj_start),
            domain: loose_string_after_key(object, "domain"),
            name: loose_string_after_key(object, "name"),
            xoxc,
        });
    }

    out
}

/// Read a token starting at `start` (the `x` of `xoxc-`), consuming the token
/// charset `[A-Za-z0-9-]`.
fn read_token(bytes: &[u8], start: usize) -> String {
    let mut end = start;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'-') {
        end += 1;
    }
    String::from_utf8_lossy(&bytes[start..end]).into_owned()
}

/// Find the byte range of the object that directly encloses `pos` using a
/// brace-balancing walk in both directions. Best-effort: quoting is ignored, so
/// braces inside strings can skew the result, but Slack team blobs are regular
/// enough that this reliably captures the team object.
fn enclosing_object(bytes: &[u8], pos: usize) -> (usize, usize) {
    // Walk backward to the nearest unmatched `{`.
    let mut depth = 0usize;
    let mut start = pos;
    let mut i = pos;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b'}' => depth += 1,
            b'{' => {
                if depth == 0 {
                    start = i;
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
    }

    // Walk forward to its matching `}`. Guard against unbalanced input (a `}`
    // seen before any `{`) so we never underflow the unsigned depth counter.
    let mut depth = 0usize;
    let mut end = pos;
    let mut j = start;
    while j < bytes.len() {
        match bytes[j] {
            b'{' => depth += 1,
            b'}' => {
                if depth <= 1 {
                    end = j;
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
        j += 1;
    }

    (start, end)
}

/// Look backward up to 512 bytes from `before` for a `T…` team-id key of the
/// form `"T0123ABCD":`, returning the id if found.
fn find_team_id_before(text: &str, before: usize) -> Option<String> {
    let start = before.saturating_sub(512);
    let region = text.get(start..before)?;
    let bytes = region.as_bytes();

    let mut i = bytes.len();
    while i > 0 {
        i -= 1;
        if bytes[i] != b'T' {
            continue;
        }
        // The `T` must open a quoted key.
        if i > 0 && bytes[i - 1] != b'"' {
            continue;
        }
        let mut j = i + 1;
        while j < bytes.len() && (bytes[j].is_ascii_uppercase() || bytes[j].is_ascii_digit()) {
            j += 1;
        }
        let id = &region[i..j];
        // Slack team ids are `T` + at least 7 base-36-ish chars.
        if id.len() >= 8 && id.len() <= 16 {
            return Some(id.to_string());
        }
    }
    None
}

/// Extract the string value following `"key"` (or the escaped `\"key\"`) within
/// `text`. Handles both plain (`"key":"val"`) and backslash-escaped
/// (`\"key\":\"val\"`) JSON representations.
fn loose_string_after_key(text: &str, key: &str) -> Option<String> {
    let kpos = text.find(key)?;
    let after = &text[kpos + key.len()..];

    let colon = after.find(':')?;
    let rest = &after[colon + 1..];

    let rb = rest.as_bytes();
    let oq = rest.find('"')?;
    // If the opening quote is preceded by a backslash, the value is embedded in
    // an escaped JSON string and is terminated by `\"` rather than `"`.
    let escaped = oq > 0 && rb[oq - 1] == b'\\';
    let value_body = &rest[oq + 1..];

    let raw = if escaped {
        let term = value_body.find("\\\"")?;
        &value_body[..term]
    } else {
        let term = value_body.find('"')?;
        &value_body[..term]
    };

    Some(unescape(raw))
}

/// Minimal JSON string unescaping for the handful of sequences Slack metadata
/// realistically contains.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('/') => out.push('/'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Wrap arbitrary payload bytes with some leading/trailing binary noise to
    /// mimic LevelDB framing around a value.
    fn framed(payload: &[u8]) -> Vec<u8> {
        let mut v = vec![0x01, 0x00, 0xff, 0x2a, 0x00, 0x13];
        v.extend_from_slice(payload);
        v.extend_from_slice(&[0x00, 0x07, 0xfe]);
        v
    }

    const SAMPLE_JSON: &str = r#"{"teams":{"T0123ABCD":{"name":"Acme Corp","domain":"acme","token":"xoxc-1111-2222-3333-abcdef0123"},"T9999ZZZZ":{"name":"Beta Inc","domain":"beta","token":"xoxc-4444-5555-6666-fedcba9876"}},"lastActiveTeamId":"T0123ABCD"}"#;

    #[test]
    fn parses_plain_localconfig_from_log_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut buf = Vec::new();
        buf.extend_from_slice(b"somekeyprefix\x00localConfig_v2");
        buf.extend_from_slice(&framed(SAMPLE_JSON.as_bytes()));
        fs::write(dir.path().join("000003.log"), &buf).unwrap();

        let mut tokens = extract_tokens_from_leveldb(dir.path()).unwrap();
        tokens.sort_by(|a, b| a.xoxc.cmp(&b.xoxc));
        assert_eq!(tokens.len(), 2);

        assert_eq!(tokens[0].xoxc, "xoxc-1111-2222-3333-abcdef0123");
        assert_eq!(tokens[0].domain.as_deref(), Some("acme"));
        assert_eq!(tokens[0].name.as_deref(), Some("Acme Corp"));
        assert_eq!(tokens[0].team_id.as_deref(), Some("T0123ABCD"));

        assert_eq!(tokens[1].xoxc, "xoxc-4444-5555-6666-fedcba9876");
        assert_eq!(tokens[1].domain.as_deref(), Some("beta"));
        assert_eq!(tokens[1].name.as_deref(), Some("Beta Inc"));
        assert_eq!(tokens[1].team_id.as_deref(), Some("T9999ZZZZ"));
    }

    #[test]
    fn newest_file_wins_for_renamed_workspace() {
        use std::time::{Duration, SystemTime};
        // Same team_id with two tokens/names across two files: a stale write
        // and a newer one, mirroring a workspace rename LevelDB has not yet
        // compacted. The newest file must be parsed first so the current record
        // (here "antiburn") leads and downstream team_id dedup keeps it.
        let old_json = concat!(
            "{\"teams\":{\"T04U8BDD0KC\":{\"name\":\"Cadence\",",
            "\"domain\":\"cadence-app\",\"token\":\"xoxc-100-200-300-oldstaletoken\"}}}"
        );
        let new_json = concat!(
            "{\"teams\":{\"T04U8BDD0KC\":{\"name\":\"antiburn\",",
            "\"domain\":\"cadence-app\",\"token\":\"xoxc-100-200-400-freshlivetoken\"}}}"
        );

        let dir = tempfile::tempdir().unwrap();
        let mut old_buf = b"\x00\x00localConfig_v2".to_vec();
        old_buf.extend_from_slice(old_json.as_bytes());
        let old_path = dir.path().join("000005.ldb");
        fs::write(&old_path, &old_buf).unwrap();

        let mut new_buf = b"\x00\x00localConfig_v2".to_vec();
        new_buf.extend_from_slice(new_json.as_bytes());
        let new_path = dir.path().join("000009.ldb");
        fs::write(&new_path, &new_buf).unwrap();

        // Make the stale file the OLDER one by mtime (std, no extra deps).
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        fs::File::options()
            .write(true)
            .open(&old_path)
            .unwrap()
            .set_modified(base)
            .unwrap();
        fs::File::options()
            .write(true)
            .open(&new_path)
            .unwrap()
            .set_modified(base + Duration::from_secs(3600))
            .unwrap();

        let tokens = extract_tokens_from_leveldb(dir.path()).unwrap();
        // Both tokens are recovered (different xoxc values, so no merge)...
        assert_eq!(tokens.len(), 2);
        // ...but the freshest write leads, so a first-seen dedup by team_id
        // keeps "antiburn".
        assert_eq!(tokens[0].name.as_deref(), Some("antiburn"));
        assert_eq!(tokens[0].xoxc, "xoxc-100-200-400-freshlivetoken");
    }

    #[test]
    fn parses_plain_json_stored_in_ldb_without_valid_sstable() {
        // A `.ldb` file that isn't a real SSTable must still be scanned raw.
        let dir = tempfile::tempdir().unwrap();
        let mut buf = b"\x00\x00localConfig_v2".to_vec();
        buf.extend_from_slice(SAMPLE_JSON.as_bytes());
        fs::write(dir.path().join("000005.ldb"), &buf).unwrap();

        let tokens = extract_tokens_from_leveldb(dir.path()).unwrap();
        assert_eq!(tokens.len(), 2);
    }

    #[test]
    fn fallback_scan_handles_escaped_json_string() {
        // localStorage stores values as escaped JSON strings; strict parsing of
        // the outer object fails, so the resilient scan must recover the token.
        let escaped = r#"blob\"teams\":{\"T555AAA00\":{\"name\":\"Gamma\",\"domain\":\"gamma\",\"token\":\"xoxc-esc-7777-8888-deadbeef00\"}}trailer"#;
        let toks = scan_teams(escaped);
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].xoxc, "xoxc-esc-7777-8888-deadbeef00");
        assert_eq!(toks[0].domain.as_deref(), Some("gamma"));
        assert_eq!(toks[0].name.as_deref(), Some("Gamma"));
        assert_eq!(toks[0].team_id.as_deref(), Some("T555AAA00"));
    }

    #[test]
    fn recovers_tokens_from_utf16le_values() {
        // Interleave a NUL after each ASCII byte to emulate UTF-16LE encoding.
        let dir = tempfile::tempdir().unwrap();
        let ascii = format!("localConfig_v2{}", SAMPLE_JSON);
        let mut utf16 = Vec::new();
        for b in ascii.bytes() {
            utf16.push(b);
            utf16.push(0x00);
        }
        fs::write(dir.path().join("000009.ldb"), &utf16).unwrap();

        let tokens = extract_tokens_from_leveldb(dir.path()).unwrap();
        let xoxcs: Vec<&str> = tokens.iter().map(|t| t.xoxc.as_str()).collect();
        assert!(xoxcs.contains(&"xoxc-1111-2222-3333-abcdef0123"));
        assert!(xoxcs.contains(&"xoxc-4444-5555-6666-fedcba9876"));
    }

    #[test]
    fn deduplicates_tokens_across_files_and_merges_metadata() {
        let dir = tempfile::tempdir().unwrap();
        // File A: full metadata.
        let mut a = b"localConfig_v2".to_vec();
        a.extend_from_slice(SAMPLE_JSON.as_bytes());
        fs::write(dir.path().join("000001.log"), &a).unwrap();
        // File B: same tokens, bare (no localConfig marker, no metadata object).
        let b = b"noise xoxc-1111-2222-3333-abcdef0123 more xoxc-4444-5555-6666-fedcba9876 end"
            .to_vec();
        fs::write(dir.path().join("000002.log"), &b).unwrap();

        let tokens = extract_tokens_from_leveldb(dir.path()).unwrap();
        assert_eq!(tokens.len(), 2, "tokens must be de-duplicated by xoxc");
        // Metadata from file A must survive the merge with file B's bare hits.
        let acme = tokens
            .iter()
            .find(|t| t.xoxc == "xoxc-1111-2222-3333-abcdef0123")
            .unwrap();
        assert_eq!(acme.domain.as_deref(), Some("acme"));
        assert_eq!(acme.name.as_deref(), Some("Acme Corp"));
    }

    #[test]
    fn missing_directory_yields_empty_vec() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");
        assert!(extract_tokens_from_leveldb(&missing).unwrap().is_empty());
    }

    #[test]
    fn varint_roundtrip_and_truncation() {
        let buf = [0xac, 0x02]; // 300
        let mut pos = 0;
        assert_eq!(read_varint(&buf, &mut pos), Some(300));
        assert_eq!(pos, 2);

        let truncated = [0x80]; // continuation bit set, no follow-up byte
        let mut pos = 0;
        assert_eq!(read_varint(&truncated, &mut pos), None);
    }

    #[test]
    fn extract_json_object_respects_string_braces() {
        let text = r#"prefix{"a":"}not-the-end{","b":1}suffix"#;
        let obj = extract_json_object(text, 0).unwrap();
        assert_eq!(obj, r#"{"a":"}not-the-end{","b":1}"#);
    }

    #[test]
    fn non_sstable_ldb_returns_none() {
        assert!(sstable_data_bytes(b"too short").is_none());
        let mut buf = vec![0u8; 64];
        // Wrong magic tail.
        buf.extend_from_slice(&[0u8; 8]);
        assert!(sstable_data_bytes(&buf).is_none());
    }
}
