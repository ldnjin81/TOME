//! Thumbnails of Unreal packages (`.uasset`, `.umap`) read without the editor.
//!
//! Only what a thumbnail needs is read: the package file summary up to its
//! `ThumbnailTableOffset`, then the thumbnail table and one thumbnail record. The layout follows
//! `FPackageFileSummary` serialization (`PackageFileSummary.cpp`, legacy file versions -7 to -9,
//! UE5 object versions to `IMPORT_TYPE_HIERARCHIES`) and `FObjectThumbnail::Serialize`. Anything
//! unexpected gives `None`, never a panic.

use serde::Serialize;

const PACKAGE_TAG: u32 = 0x9E2A_83C1;
/// `PKG_FilterEditorOnly`: cooked content, which has no localization id and no thumbnails.
const PKG_FILTER_EDITOR_ONLY: u32 = 0x8000_0000;

// UE4 object versions that gate summary fields (every UE5 package is above all of them).
const VER_UE4_ADD_STRING_ASSET_REFERENCES_MAP: i32 = 384;
const VER_UE4_SERIALIZE_TEXT_IN_PACKAGES: i32 = 459;
const VER_UE4_ADDED_SEARCHABLE_NAMES: i32 = 510;
const VER_UE4_ADDED_PACKAGE_SUMMARY_LOCALIZATION_ID: i32 = 516;
// UE5 object versions (EUnrealEngineObjectUE5Version).
const UE5_ADD_SOFTOBJECTPATH_LIST: i32 = 1008;
const UE5_METADATA_SERIALIZATION_OFFSET: i32 = 1014;
const UE5_VERSE_CELLS: i32 = 1015;
const UE5_PACKAGE_SAVED_HASH: i32 = 1016;
/// Assumed for unversioned packages (all versions zero): the newest the reader knows.
const UE5_LATEST_KNOWN: i32 = 1018;
const UE4_LATEST_KNOWN: i32 = 522;

/// A package's thumbnail: the compressed image as stored (PNG or JPEG).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Thumbnail {
    /// The class of the object it shows (`StaticMesh`, `Material`, ...).
    pub class: String,
    pub width: i32,
    pub height: i32,
    /// "image/png" or "image/jpeg".
    pub mime: &'static str,
    pub data: Vec<u8>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], at: usize) -> Self {
        Reader { bytes, at }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(n)?;
        let slice = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(slice)
    }

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn skip(&mut self, n: usize) -> Option<()> {
        self.take(n).map(|_| ())
    }

    /// An FString: a length that counts the terminating zero; negative means UTF-16.
    fn fstring(&mut self) -> Option<String> {
        let length = self.i32()?;
        match length {
            0 => Some(String::new()),
            n if n > 0 => {
                let raw = self.take(n as usize)?;
                Some(String::from_utf8_lossy(&raw[..raw.len() - 1]).into_owned())
            }
            n => {
                let units = n.checked_neg()? as usize;
                let raw = self.take(units.checked_mul(2)?)?;
                let wide: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                Some(String::from_utf16_lossy(&wide[..wide.len().saturating_sub(1)]))
            }
        }
    }
}

/// The summary fields a thumbnail needs.
struct Summary {
    thumbnail_table_offset: i32,
    editor_only_filtered: bool,
}

fn read_summary(bytes: &[u8]) -> Option<Summary> {
    let mut r = Reader::new(bytes, 0);
    if r.u32()? != PACKAGE_TAG {
        return None;
    }
    let legacy = r.i32()?;
    // -7 .. -9 are UE4.26+ and UE5; older or newer layouts are not read.
    if !(-9..=-7).contains(&legacy) {
        return None;
    }
    if legacy != -4 {
        r.i32()?; // LegacyUE3Version
    }
    let mut ue4 = r.i32()?;
    let mut ue5 = if legacy <= -8 { r.i32()? } else { 0 };
    let licensee = r.i32()?;
    if ue4 == 0 && ue5 == 0 && licensee == 0 {
        ue4 = UE4_LATEST_KNOWN;
        ue5 = UE5_LATEST_KNOWN;
    }
    if ue5 >= UE5_PACKAGE_SAVED_HASH {
        r.skip(20)?; // SavedHash (FIoHash)
        r.i32()?; // TotalHeaderSize
    }
    // Custom versions (optimized format): count, then a GUID and a version each.
    let custom = r.i32()?;
    if !(0..=10_000).contains(&custom) {
        return None;
    }
    r.skip(custom as usize * 20)?;
    if ue5 < UE5_PACKAGE_SAVED_HASH {
        r.i32()?; // TotalHeaderSize
    }
    r.fstring()?; // PackageName
    let flags = r.u32()?;
    let editor_only_filtered = flags & PKG_FILTER_EDITOR_ONLY != 0;
    r.skip(8)?; // NameCount, NameOffset
    if ue5 >= UE5_ADD_SOFTOBJECTPATH_LIST {
        r.skip(8)?;
    }
    if !editor_only_filtered && ue4 >= VER_UE4_ADDED_PACKAGE_SUMMARY_LOCALIZATION_ID {
        r.fstring()?; // LocalizationId
    }
    if ue4 >= VER_UE4_SERIALIZE_TEXT_IN_PACKAGES {
        r.skip(8)?; // GatherableTextData
    }
    r.skip(16)?; // Exports, Imports
    if ue5 >= UE5_VERSE_CELLS {
        r.skip(16)?; // CellExports, CellImports
    }
    if ue5 >= UE5_METADATA_SERIALIZATION_OFFSET {
        r.skip(4)?; // MetaDataOffset
    }
    r.skip(4)?; // DependsOffset
    if ue4 >= VER_UE4_ADD_STRING_ASSET_REFERENCES_MAP {
        r.skip(8)?; // SoftPackageReferences
    }
    if ue4 >= VER_UE4_ADDED_SEARCHABLE_NAMES {
        r.skip(4)?; // SearchableNamesOffset
    }
    let thumbnail_table_offset = r.i32()?;
    Some(Summary { thumbnail_table_offset, editor_only_filtered })
}

/// The summary's thumbnail table offset (0: the package stores no thumbnails); None when the
/// file is not a package the reader understands.
pub fn thumbnail_table_offset(bytes: &[u8]) -> Option<i32> {
    read_summary(bytes).map(|s| s.thumbnail_table_offset)
}

/// What the thumbnail table says about a package: the asset's class, and its image when one is
/// stored (blueprints, sounds and maps often keep an empty 0x0 thumbnail the editor draws live).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Preview {
    pub class: String,
    pub thumbnail: Option<Thumbnail>,
}

/// Where the package bytes come from: all in memory, or read piece by piece from a file.
trait Source {
    /// `n` bytes at `at`, or fewer at the end; None when `at` is past the end or reading fails.
    fn read(&mut self, at: u64, n: usize) -> Option<Vec<u8>>;
}

impl Source for &[u8] {
    fn read(&mut self, at: u64, n: usize) -> Option<Vec<u8>> {
        let at = usize::try_from(at).ok()?;
        (at <= self.len()).then(|| self[at..self.len().min(at.saturating_add(n))].to_vec())
    }
}

impl Source for std::fs::File {
    fn read(&mut self, at: u64, n: usize) -> Option<Vec<u8>> {
        use std::io::{Read, Seek, SeekFrom};
        self.seek(SeekFrom::Start(at)).ok()?;
        let mut out = Vec::with_capacity(n.min(1 << 20));
        self.by_ref().take(n as u64).read_to_end(&mut out).ok()?;
        Some(out)
    }
}

/// The summary fits in this much (name, custom versions and the fields before the table offset).
const SUMMARY_BYTES: usize = 256 * 1024;
/// A thumbnail table entry: count, class name and object path, and the record offset.
const TABLE_ENTRY_BYTES: usize = 4096;
/// Larger thumbnails are not real (the editor saves 256x256 images).
const MAX_THUMBNAIL_BYTES: i32 = 16 << 20;

fn read_preview(source: &mut impl Source) -> Option<Preview> {
    let summary = read_summary(&source.read(0, SUMMARY_BYTES)?)?;
    if summary.editor_only_filtered || summary.thumbnail_table_offset <= 0 {
        return None;
    }
    let entry = source.read(summary.thumbnail_table_offset as u64, TABLE_ENTRY_BYTES)?;
    let mut table = Reader::new(&entry, 0);
    let count = table.i32()?;
    if !(1..=1000).contains(&count) {
        return None;
    }
    // A package has one asset; take the first thumbnail.
    let class = table.fstring()?;
    table.fstring()?; // ObjectPathWithoutPackageName
    let offset = u64::try_from(table.i32()?).ok()?;
    let thumbnail = (|| {
        let header = source.read(offset, 12)?;
        let mut record = Reader::new(&header, 0);
        let (width, height, size) = (record.i32()?, record.i32()?, record.i32()?);
        if width <= 0 || height == 0 || !(1..=MAX_THUMBNAIL_BYTES).contains(&size) {
            return None;
        }
        let data = source.read(offset + 12, size as usize)?;
        if data.len() != size as usize {
            return None;
        }
        // A negative height marks JPEG data; check the bytes too.
        let mime = if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            "image/jpeg"
        } else if data.starts_with(b"\x89PNG\r\n\x1a\n") {
            "image/png"
        } else {
            return None;
        };
        Some(Thumbnail { class: class.clone(), width, height: height.abs(), mime, data })
    })();
    Some(Preview { class, thumbnail })
}

/// The package's class and thumbnail; None when it is not a package the reader understands or
/// it has no thumbnail table (cooked packages, data assets saved without one).
pub fn preview(mut bytes: &[u8]) -> Option<Preview> {
    read_preview(&mut bytes)
}

/// [`preview`] of a package file, reading only the summary, the table entry and the image
/// rather than the whole file (maps and meshes can be hundreds of megabytes).
pub fn preview_file(path: impl AsRef<std::path::Path>) -> Option<Preview> {
    read_preview(&mut std::fs::File::open(path).ok()?)
}

/// The package's thumbnail image, if one is stored.
pub fn thumbnail(bytes: &[u8]) -> Option<Thumbnail> {
    preview(bytes)?.thumbnail
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fstring(out: &mut Vec<u8>, text: &str) {
        out.extend(((text.len() + 1) as i32).to_le_bytes());
        out.extend(text.as_bytes());
        out.push(0);
    }

    fn wide(out: &mut Vec<u8>, text: &str) {
        let units: Vec<u16> = text.encode_utf16().chain([0]).collect();
        out.extend((-(units.len() as i32)).to_le_bytes());
        for u in units {
            out.extend(u.to_le_bytes());
        }
    }

    /// A package as UE 5.8 saves it (legacy -9, UE5 1018), with a thumbnail of `image`.
    fn package(image: &[u8], height: i32, flags: u32) -> Vec<u8> {
        package_with(image, height, flags, true)
    }

    fn package_with(image: &[u8], height: i32, flags: u32, with_table: bool) -> Vec<u8> {
        let mut b = Vec::new();
        let i = |b: &mut Vec<u8>, v: i32| b.extend(v.to_le_bytes());
        b.extend(PACKAGE_TAG.to_le_bytes());
        i(&mut b, -9); // legacy
        i(&mut b, 864); // legacy UE3
        i(&mut b, 522); // UE4
        i(&mut b, 1018); // UE5
        i(&mut b, 0); // licensee
        b.extend([7u8; 20]); // saved hash
        i(&mut b, 0); // total header size
        i(&mut b, 2); // custom versions
        b.extend([1u8; 40]);
        wide(&mut b, "/Game/Props/SM_Chair");
        b.extend(flags.to_le_bytes());
        for _ in 0..4 {
            i(&mut b, 0); // names, soft object paths
        }
        if flags & PKG_FILTER_EDITOR_ONLY == 0 {
            fstring(&mut b, "ABCDEF");
        }
        for _ in 0..2 + 4 + 4 + 1 + 1 + 2 + 1 {
            i(&mut b, 0); // gatherable text, exports/imports, cells, metadata, depends, soft refs, searchable names
        }
        let table_at = b.len() + 4 + 64; // past the offset field and some padding
        i(&mut b, if with_table { table_at as i32 } else { 0 });
        b.resize(table_at, 0);
        i(&mut b, 1);
        fstring(&mut b, "StaticMesh");
        fstring(&mut b, "SM_Chair");
        let record_at = b.len() + 4;
        i(&mut b, record_at as i32);
        i(&mut b, 256);
        i(&mut b, height);
        i(&mut b, image.len() as i32);
        b.extend(image);
        b
    }

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nrest-of-png";
    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3];

    #[test]
    fn reads_png_and_jpeg_thumbnails() {
        let png = thumbnail(&package(PNG, 256, 0)).expect("png thumbnail");
        assert_eq!((png.class.as_str(), png.width, png.height, png.mime), ("StaticMesh", 256, 256, "image/png"));
        assert_eq!(png.data, PNG);
        // JPEG is stored with a negative height.
        let jpeg = thumbnail(&package(JPEG, -128, 0)).expect("jpeg thumbnail");
        assert_eq!((jpeg.height, jpeg.mime), (128, "image/jpeg"));
    }

    #[test]
    fn no_thumbnail_without_a_table_or_for_cooked_packages() {
        let cooked = package(PNG, 256, PKG_FILTER_EDITOR_ONLY);
        assert_eq!(thumbnail(&cooked), None);
        assert_eq!(thumbnail(&package_with(PNG, 256, 0, false)), None);
    }

    #[test]
    fn an_empty_thumbnail_still_names_the_class() {
        // Blueprints keep a 0x0 thumbnail: no image, but the class is known.
        let mut b = package(PNG, 256, 0);
        let image_at = b.len() - PNG.len() - 12;
        b[image_at..image_at + 12].copy_from_slice(&[0u8; 12]);
        b.truncate(image_at + 12);
        assert_eq!(thumbnail(&b), None);
        assert_eq!(preview(&b), Some(Preview { class: "StaticMesh".into(), thumbnail: None }));
        assert_eq!(preview(&package(PNG, 256, 0)).unwrap().thumbnail.unwrap().mime, "image/png");
    }

    #[test]
    fn garbage_and_truncated_input_give_none() {
        assert_eq!(thumbnail(b""), None);
        assert_eq!(thumbnail(b"not a package at all"), None);
        let full = package(PNG, 256, 0);
        for cut in [4, 20, 60, full.len() / 2, full.len() - 3] {
            assert_eq!(thumbnail(&full[..cut]), None, "cut at {cut}");
        }
        // Image bytes that are neither PNG nor JPEG.
        assert_eq!(thumbnail(&package(b"BMP?", 256, 0)), None);
    }

    #[test]
    fn a_file_reads_the_same_as_its_bytes() {
        let dir = std::env::temp_dir().join(format!("tome-uasset-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, bytes) in [("a.uasset", package(PNG, 256, 0)), ("b.uasset", package_with(PNG, 256, 0, false)), ("c.uasset", b"junk".to_vec())] {
            std::fs::write(dir.join(name), &bytes).unwrap();
            assert_eq!(preview_file(dir.join(name)), preview(&bytes), "{name}");
        }
        assert_eq!(preview_file(dir.join("missing.uasset")), None);
        std::fs::remove_dir_all(&dir).ok();
    }
}
