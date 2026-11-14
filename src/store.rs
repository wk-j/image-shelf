use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    fs, io,
    path::{Path, PathBuf},
};
pub const MAX_IMAGE: usize = 32 * 1024 * 1024;
pub const MAX_TOTAL: u64 = 512 * 1024 * 1024;
pub const MAX_ITEMS: usize = 200;

/// History PNGs with their byte sizes, newest first. One stat per file.
pub fn scan(dir: &Path) -> io::Result<Vec<(PathBuf, u64)>> {
    fs::create_dir_all(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    let mut files: Vec<_> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "png"))
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            m.is_file().then(|| (e.path(), m.len(), m.modified().ok()))
        })
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.2));
    Ok(files.into_iter().map(|(p, len, _)| (p, len)).collect())
}
/// Returns the stored path and, for a new file, its decoded pixels so the caller
/// can thumbnail without decoding again.
pub fn save(dir: &Path, png: &[u8]) -> io::Result<(PathBuf, Option<image::DynamicImage>)> {
    if png.len() > MAX_IMAGE {
        return Err(io::Error::other("Image exceeds 32 MB limit"));
    }
    let clip =
        rune::ClipboardImage::from_png(png).ok_or_else(|| io::Error::other("Invalid PNG"))?;
    if clip.size().0 as u64 * clip.size().1 as u64 > 40_000_000 {
        return Err(io::Error::other("Image exceeds 40 megapixels"));
    }
    let path = dir.join(format!("{}.png", blake3::hash(png).to_hex()));
    let files = scan(dir)?;
    if path.is_file() {
        return Ok((path, None));
    }
    let decoded = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(io::Error::other)?;
    if decoded.width() as u64 * decoded.height() as u64 > 40_000_000 {
        return Err(io::Error::other("Image exceeds 40 megapixels"));
    }
    let total: u64 = files.iter().map(|f| f.1).sum();
    if files.len() >= MAX_ITEMS || total + png.len() as u64 > MAX_TOTAL {
        return Err(io::Error::other(
            "Shelf full (200 images / 512 MB). Open folder to manage history.",
        ));
    }
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    f.write_all(png)?;
    Ok((path, Some(decoded)))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saves_deduplicates_and_reopens() {
        let dir = std::env::temp_dir().join(format!("image-shelf-test-{}", std::process::id()));
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(4, 3)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let (p, decoded) = save(&dir, png.get_ref()).unwrap();
        assert!(decoded.is_some());
        let (again, decoded) = save(&dir, png.get_ref()).unwrap();
        assert_eq!(p, again);
        assert!(decoded.is_none(), "duplicate skips decoding");
        assert_eq!(scan(&dir).unwrap().len(), 1);
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(save(&dir, b"not png").is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
