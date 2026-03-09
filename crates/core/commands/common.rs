use std::io::Write;
use std::path::Path;

use tempfile::NamedTempFile;

use crate::document::Document;
use crate::error::LinehashError;

pub fn check_guard(
    doc: &Document,
    expect_mtime: Option<i64>,
    expect_inode: Option<u64>,
) -> Result<(), LinehashError> {
    let Some(meta) = &doc.file_meta else {
        return Ok(());
    };

    if expect_mtime.is_some_and(|expected| expected != meta.mtime_secs)
        || expect_inode.is_some_and(|expected| expected != meta.inode)
    {
        return Err(LinehashError::StaleFile {
            path: doc.path.display().to_string(),
        });
    }

    Ok(())
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), LinehashError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.flush()?;
    temp.persist(path)
        .map(|_| ())
        .map_err(|error| LinehashError::Io(error.error))
}
