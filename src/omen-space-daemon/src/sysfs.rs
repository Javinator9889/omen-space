/// Writers for kernel attribute files (sysfs, procfs, debugfs).
///
/// `std::fs::write` goes through `write_all`, which trusts the byte count returned by
/// `write(2)` and slices its buffer with it (`&buf[n..]`). A sysfs `store` callback is
/// supposed to return the number of bytes consumed, but out-of-tree and vendor drivers
/// sometimes return a firmware status code instead. `hp-omen-extra` did exactly that on
/// boards without a keyboard backlight (BIOS code 3 returned for a 1-byte write), which
/// turned into `range start index 3 out of range for slice of length 1` and crashed the
/// daemon on startup (issue #253, OMEN 45L / board 8949).
///
/// Attribute files take the whole value in a single `write(2)` call, so the right
/// semantics are: write once, treat any non-error return as success, ignore the count.
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Write `contents` to a kernel attribute with a single `write(2)`.
pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).open(path)?;
    // The returned count is deliberately ignored: see the module docs.
    #[allow(clippy::unused_io_amount)]
    file.write(contents.as_ref())?;
    Ok(())
}

/// Async wrapper for [`write`]; the blocking write runs on tokio's blocking pool.
pub async fn write_async(path: impl Into<PathBuf>, contents: impl Into<Vec<u8>>) -> io::Result<()> {
    let path = path.into();
    let contents = contents.into();
    tokio::task::spawn_blocking(move || write(path, contents))
        .await
        .map_err(io::Error::other)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_succeeds_on_existing_node_and_never_creates_one() {
        let path = std::env::temp_dir().join(format!("omen-sysfs-test-{}", std::process::id()));
        std::fs::write(&path, "").unwrap();
        write(&path, "1").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "1");
        std::fs::remove_file(&path).unwrap();
        // Attributes are opened without O_CREAT, so a missing node is an error, not a new file.
        assert!(write(&path, "1").is_err());
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn write_async_reports_missing_node() {
        assert!(write_async("/nonexistent/omen/attr", "1").await.is_err());
    }
}
