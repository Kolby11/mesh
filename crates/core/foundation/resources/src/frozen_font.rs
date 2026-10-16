use crate::{
    ResourceAssetError, ResourceByteBudget, ResourceByteReservation, ResourcePreparationToken,
};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// An owned font database whose sources never reopen files or shared mappings.
/// Face metadata, collection indexes and generic-family choices are preserved;
/// consumers with existing face IDs must translate them through `face_id`.
#[derive(Clone)]
pub struct FrozenFontDatabase {
    database: Arc<fontdb::Database>,
    face_ids: Arc<HashMap<fontdb::ID, fontdb::ID>>,
    bytes: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum FrozenFontError {
    #[error("font snapshot exceeds the shared resource byte budget")]
    Budget,
    #[error("font snapshot preparation was cancelled")]
    Cancelled,
    #[error("font changed during snapshot preparation: {0}")]
    Changed(PathBuf),
    #[error(transparent)]
    Asset(#[from] ResourceAssetError),
}

#[derive(Hash, PartialEq, Eq)]
enum SourceKey {
    File(PathBuf),
    Memory(usize),
}

struct FrozenFontBytes {
    data: Box<[u8]>,
    _reservation: ResourceByteReservation,
}

impl AsRef<[u8]> for FrozenFontBytes {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl FrozenFontDatabase {
    /// Copy font sources under a shared byte budget, checking cancellation
    /// between bounded read/copy chunks. File I/O belongs on a preparation
    /// worker. Failure drops the partial snapshot and all its reservations.
    /// The caller must publish this database to layout and painting together.
    pub fn prepare(
        source: &fontdb::Database,
        budget: &ResourceByteBudget,
        cancellation: &ResourcePreparationToken,
    ) -> Result<Self, FrozenFontError> {
        let mut database = fontdb::Database::new();
        database.set_serif_family(source.family_name(&fontdb::Family::Serif));
        database.set_sans_serif_family(source.family_name(&fontdb::Family::SansSerif));
        database.set_cursive_family(source.family_name(&fontdb::Family::Cursive));
        database.set_fantasy_family(source.family_name(&fontdb::Family::Fantasy));
        database.set_monospace_family(source.family_name(&fontdb::Family::Monospace));
        let mut sources = HashMap::<SourceKey, Arc<FrozenFontBytes>>::new();
        let mut face_ids = HashMap::new();
        let mut bytes = 0;
        for face in source.faces() {
            check_cancelled(cancellation)?;
            let key = match &face.source {
                fontdb::Source::File(path) => SourceKey::File(path.clone()),
                fontdb::Source::Binary(data) | fontdb::Source::SharedFile(_, data) => {
                    SourceKey::Memory(Arc::as_ptr(data) as *const () as usize)
                }
            };
            let data = if let Some(data) = sources.get(&key) {
                Arc::clone(data)
            } else {
                let data = Arc::new(match &face.source {
                    fontdb::Source::File(path) => freeze_file(path, budget, cancellation)?,
                    fontdb::Source::Binary(data) | fontdb::Source::SharedFile(_, data) => {
                        freeze_memory(data.as_ref().as_ref(), budget, cancellation)?
                    }
                });
                bytes += data.data.len();
                sources.insert(key, Arc::clone(&data));
                data
            };
            let mut frozen = face.clone();
            frozen.source = fontdb::Source::Binary(data);
            let id = database.push_face_info(frozen);
            face_ids.insert(face.id, id);
        }
        check_cancelled(cancellation)?;
        Ok(Self {
            database: Arc::new(database),
            face_ids: Arc::new(face_ids),
            bytes,
        })
    }

    /// Clone metadata while retaining the immutable, budgeted byte sources.
    /// Reservations live until the last database/source clone is dropped.
    pub fn database(&self) -> fontdb::Database {
        self.database.as_ref().clone()
    }

    /// Translate an original face ID; unknown or removed IDs return `None`.
    pub fn face_id(&self, original: fontdb::ID) -> Option<fontdb::ID> {
        self.face_ids.get(&original).copied()
    }

    /// Unique source bytes retained by this snapshot, excluding metadata.
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

fn check_cancelled(token: &ResourcePreparationToken) -> Result<(), FrozenFontError> {
    if token.is_cancelled() {
        Err(FrozenFontError::Cancelled)
    } else {
        Ok(())
    }
}

fn freeze_memory(
    input: &[u8],
    budget: &ResourceByteBudget,
    cancellation: &ResourcePreparationToken,
) -> Result<FrozenFontBytes, FrozenFontError> {
    let reservation = budget
        .try_reserve(input.len())
        .ok_or(FrozenFontError::Budget)?;
    let mut data = Vec::with_capacity(input.len());
    for chunk in input.chunks(crate::RESOURCE_READ_CHUNK_BYTES) {
        check_cancelled(cancellation)?;
        data.extend_from_slice(chunk);
    }
    check_cancelled(cancellation)?;
    Ok(FrozenFontBytes {
        data: data.into_boxed_slice(),
        _reservation: reservation,
    })
}

fn freeze_file(
    path: &Path,
    budget: &ResourceByteBudget,
    cancellation: &ResourcePreparationToken,
) -> Result<FrozenFontBytes, FrozenFontError> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let mut file = options
        .open(path)
        .map_err(|source| ResourceAssetError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    let metadata = file.metadata().map_err(|source| ResourceAssetError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.file_type().is_file() {
        return Err(ResourceAssetError::NotRegularFile {
            path: path.to_path_buf(),
        }
        .into());
    }
    let len = usize::try_from(metadata.len()).map_err(|_| FrozenFontError::Budget)?;
    let reservation = budget.try_reserve(len).ok_or(FrozenFontError::Budget)?;
    let mut data = Vec::with_capacity(len);
    let mut chunk = [0; crate::RESOURCE_READ_CHUNK_BYTES];
    while data.len() < len {
        check_cancelled(cancellation)?;
        let amount = (len - data.len()).min(chunk.len());
        let read = file
            .read(&mut chunk[..amount])
            .map_err(|source| ResourceAssetError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        if read == 0 {
            break;
        }
        data.extend_from_slice(&chunk[..read]);
    }
    check_cancelled(cancellation)?;
    let extra = file
        .read(&mut chunk[..1])
        .map_err(|source| ResourceAssetError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    if data.len() != len || extra != 0 {
        return Err(FrozenFontError::Changed(path.to_path_buf()));
    }
    let after = file.metadata().map_err(|source| ResourceAssetError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.len() != after.len() || metadata.modified().ok() != after.modified().ok() {
        return Err(FrozenFontError::Changed(path.to_path_buf()));
    }
    Ok(FrozenFontBytes {
        data: data.into_boxed_slice(),
        _reservation: reservation,
    })
}

#[cfg(test)]
mod tests;
