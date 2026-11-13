use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    fs, io,
    path::{Path, PathBuf},
};
pub const MAX_IMAGE: usize = 32 * 1024 * 1024;
pub const MAX_TOTAL: u64 = 512 * 1024 * 1024;
pub const MAX_ITEMS: usize = 200;

pub fn list(dir: &Path) -> io::Result<Vec<PathBuf>> {
    fs::create_dir_all(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    let mut files: Vec<_> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_type().is_ok_and(|t| t.is_file())
                && e.path().extension().is_some_and(|x| x == "png")
        })
        .map(|e| e.path())
        .collect();
    files.sort_by_key(|p| std::cmp::Reverse(fs::metadata(p).and_then(|m| m.modified()).ok()));
    Ok(files)
}
pub fn save(dir: &Path, png: &[u8]) -> io::Result<PathBuf> {
    if png.len() > MAX_IMAGE {
        return Err(io::Error::other("Image exceeds 32 MB limit"));
    }
    let clip =
        rune::ClipboardImage::from_png(png).ok_or_else(|| io::Error::other("Invalid PNG"))?;
    if clip.size().0 as u64 * clip.size().1 as u64 > 40_000_000 {
        return Err(io::Error::other("Image exceeds 40 megapixels"));
    }
    let decoded = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(io::Error::other)?;
    if decoded.width() as u64 * decoded.height() as u64 > 40_000_000 {
        return Err(io::Error::other("Image exceeds 40 megapixels"));
    }
    let path = dir.join(format!("{}.png", blake3::hash(png).to_hex()));
    let files = list(dir)?;
    if path.is_file() {
        return Ok(path);
    }
    let total: u64 = files
        .iter()
        .filter_map(|p| fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();
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
    Ok(path)
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
        let p = save(&dir, png.get_ref()).unwrap();
        assert_eq!(p, save(&dir, png.get_ref()).unwrap());
        assert_eq!(list(&dir).unwrap().len(), 1);
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(save(&dir, b"not png").is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
