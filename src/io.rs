pub mod riff;

/// Data that has been read from a file newer than this
/// version supports, but is marked by the writer as keepable.
pub struct OrphanedData {
    /// TODO: keep track of from where in the RIFF tree this
    /// node belongs. It must have the same parent as it originally had,
    /// but may be placed in any index within that parent.
    position: (),
    id: riff::ChunkID,
    version: Version,
    /// Entire data of the chunk, including header.
    data: Vec<u8>,
}

#[derive(thiserror::Error, Debug)]
pub enum WriteError {
    #[error("{}", .0)]
    IO(std::io::Error),
    #[error("{}", .0)]
    Anyhow(anyhow::Error),
}
impl From<std::io::Error> for WriteError {
    fn from(value: std::io::Error) -> Self {
        Self::IO(value)
    }
}
impl From<anyhow::Error> for WriteError {
    fn from(value: anyhow::Error) -> Self {
        Self::Anyhow(value)
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
#[repr(u8)]
pub enum OrphanMode {
    Keep = 0,
    Discard = 1,
    Deny = 2,
}
impl OrphanMode {
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::Keep),
            1 => Some(Self::Discard),
            2 => Some(Self::Deny),
            _ => None,
        }
    }
}
#[derive(bytemuck::Pod, bytemuck::Zeroable, Clone, Copy)]
#[repr(C, packed)]
pub struct DictMetadata<InnerMeta: bytemuck::Pod + bytemuck::Zeroable + Copy> {
    pub offset: u32,
    pub len: u32,
    pub inner: InnerMeta,
}
#[derive(bytemuck::Pod, bytemuck::Zeroable, Clone, Copy)]
#[repr(C)]
pub struct Version(pub u8, pub u8, pub u8);
impl Version {
    pub const CURRENT: Self = Version(0, 0, 0);
}
#[repr(C)]
pub struct VersionedChunkHeader(Version, OrphanMode);
/// From the given document state reader and repository handle, write a `.fzp` document into the given writer.
pub fn write_into<Document, Writer>(
    document: Document,
    point_repository: &crate::repositories::points::PointRepository,
    writer: Writer,
) -> Result<(), WriteError>
where
    Document: crate::commands::queue::state_reader::CommandQueueStateReader,
    Writer: std::io::Write + std::io::Seek,
{
    use riff::*;
    use std::io::Write;
    let mut root = BinaryChunkWriter::new_subtype(writer, ChunkID::RIFF, ChunkID::FZP_)?;
    {
        {
            let mut info = BinaryChunkWriter::new_subtype(&mut root, ChunkID::LIST, ChunkID::INFO)?;
            BinaryChunkWriter::new(&mut info, ChunkID(*b"ISFT"))?.write_all(b"fuzzpaint")?;
        }
        let _ = BinaryChunkWriter::new(&mut root, ChunkID::DOCV)?;
        let _ = BinaryChunkWriter::new(&mut root, ChunkID::GRPH)?;
        let _ = BinaryChunkWriter::new(&mut root, ChunkID::HIST)?;
        {
            let collections = document.stroke_collections();
            point_repository
                .write_dict_into(
                    collections
                        .0
                        .iter()
                        .flat_map(|collection| collection.1.strokes.iter())
                        .map(|stroke| stroke.point_collection),
                    &mut root,
                )
                .map_err(|err| -> anyhow::Error { err.into() })?;
        }
        let _ = BinaryChunkWriter::new_subtype(&mut root, ChunkID::DICT, ChunkID::BRSH)?;
    }

    Ok(())
}
