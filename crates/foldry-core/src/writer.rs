use std::{
    fmt,
    fs::File,
    io::{self, Read, Write},
};

use flate2::{Compression, write::GzEncoder};
use sevenz_rust2::{
    ArchiveEntry as SevenZipEntry, ArchiveWriter as SevenZipWriter, EncoderConfiguration,
    EncoderMethod, encoder_options::Lzma2Options,
};
use tar::{Builder, EntryType, Header};
use zip::{CompressionMethod, ZipWriter, write::FullFileOptions};

use crate::{ArchiveFormat, CompressionLevel, ScannedEntry};

#[derive(Debug)]
pub struct ArchiveWriteError {
    pub message: String,
}

impl ArchiveWriteError {
    fn new(error: impl fmt::Display) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

impl fmt::Display for ArchiveWriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ArchiveWriteError {}

impl From<io::Error> for ArchiveWriteError {
    fn from(error: io::Error) -> Self {
        Self::new(error)
    }
}

/// Format-independent archive entry writer.
pub trait ArchiveWriterBackend {
    fn add_directory(
        &mut self,
        archive_path: &str,
        entry: &ScannedEntry,
    ) -> Result<(), ArchiveWriteError>;
    fn add_file(
        &mut self,
        archive_path: &str,
        entry: &ScannedEntry,
        reader: &mut dyn Read,
    ) -> Result<(), ArchiveWriteError>;
    fn add_symlink(
        &mut self,
        archive_path: &str,
        entry: &ScannedEntry,
    ) -> Result<(), ArchiveWriteError>;
    fn finish(self: Box<Self>) -> Result<File, ArchiveWriteError>;
}

pub fn create_archive_writer(
    format: ArchiveFormat,
    level: CompressionLevel,
    file: File,
) -> Result<Box<dyn ArchiveWriterBackend>, ArchiveWriteError> {
    match format {
        ArchiveFormat::Zip => Ok(Box::new(ZipBackend {
            writer: Some(ZipWriter::new(file)),
            level: codec_level(format, level),
        })),
        ArchiveFormat::TarGz => Ok(Box::new(TarGzBackend {
            builder: Some(Builder::new(GzEncoder::new(
                file,
                Compression::new(codec_level(format, level) as u32),
            ))),
        })),
        ArchiveFormat::TarZst => Ok(Box::new(TarZstBackend {
            builder: Some(Builder::new(
                zstd::stream::write::Encoder::new(file, codec_level(format, level))
                    .map_err(ArchiveWriteError::new)?,
            )),
        })),
        ArchiveFormat::SevenZip => {
            let mut writer = SevenZipWriter::new(file).map_err(ArchiveWriteError::new)?;
            writer.set_content_methods(vec![
                EncoderConfiguration::new(EncoderMethod::LZMA2).with_options(
                    Lzma2Options::from_level(codec_level(format, level) as u32).into(),
                ),
            ]);
            Ok(Box::new(SevenZipBackend {
                writer: Some(writer),
            }))
        }
    }
}

#[must_use]
pub const fn codec_level(format: ArchiveFormat, level: CompressionLevel) -> i32 {
    match (format, level) {
        (ArchiveFormat::Zip | ArchiveFormat::TarGz, CompressionLevel::Fast) => 1,
        (ArchiveFormat::Zip | ArchiveFormat::TarGz, CompressionLevel::Balanced) => 6,
        (ArchiveFormat::Zip | ArchiveFormat::TarGz, CompressionLevel::Maximum) => 9,
        (ArchiveFormat::TarZst, CompressionLevel::Fast) => 1,
        (ArchiveFormat::TarZst, CompressionLevel::Balanced) => 3,
        (ArchiveFormat::TarZst, CompressionLevel::Maximum) => 19,
        (ArchiveFormat::SevenZip, CompressionLevel::Fast) => 1,
        (ArchiveFormat::SevenZip, CompressionLevel::Balanced) => 6,
        (ArchiveFormat::SevenZip, CompressionLevel::Maximum) => 9,
    }
}

struct ZipBackend {
    writer: Option<ZipWriter<File>>,
    level: i32,
}

impl ZipBackend {
    fn options(&self, entry: &ScannedEntry) -> Result<FullFileOptions<'static>, ArchiveWriteError> {
        let mut options = FullFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(i64::from(self.level)))
            .large_file(entry.size >= u64::from(u32::MAX));
        if let Some(modified) = entry.modified_unix_nanos {
            let nanos = i128::from(modified);
            if let Ok(timestamp) = time::OffsetDateTime::from_unix_timestamp_nanos(nanos)
                && let Ok(zip_time) = zip::DateTime::try_from(time::PrimitiveDateTime::new(
                    timestamp.date(),
                    timestamp.time(),
                ))
            {
                options = options.last_modified_time(zip_time);
            }
        }
        let mut extended_timestamp = Vec::with_capacity(9);
        let mut flags = 0_u8;
        if let Some(modified) = entry
            .modified_unix_nanos
            .and_then(|value| u32::try_from(value / 1_000_000_000).ok())
        {
            flags |= 0b0000_0001;
            extended_timestamp.extend_from_slice(&modified.to_le_bytes());
        }
        if let Some(created) = entry
            .created_unix_nanos
            .and_then(|value| u32::try_from(value / 1_000_000_000).ok())
        {
            flags |= 0b0000_0100;
            extended_timestamp.extend_from_slice(&created.to_le_bytes());
        }
        if flags != 0 {
            extended_timestamp.insert(0, flags);
            options
                .add_extra_data(0x5455, extended_timestamp, false)
                .map_err(ArchiveWriteError::new)?;
        }
        Ok(options)
    }

    fn writer(&mut self) -> Result<&mut ZipWriter<File>, ArchiveWriteError> {
        self.writer
            .as_mut()
            .ok_or_else(|| ArchiveWriteError::new("ZIP writer is already finished"))
    }
}

impl ArchiveWriterBackend for ZipBackend {
    fn add_directory(
        &mut self,
        archive_path: &str,
        entry: &ScannedEntry,
    ) -> Result<(), ArchiveWriteError> {
        let options = self
            .options(entry)?
            .unix_permissions(entry.unix_mode.unwrap_or(0o755));
        self.writer()?
            .add_directory(ensure_directory_path(archive_path), options)
            .map_err(ArchiveWriteError::new)
    }

    fn add_file(
        &mut self,
        archive_path: &str,
        entry: &ScannedEntry,
        reader: &mut dyn Read,
    ) -> Result<(), ArchiveWriteError> {
        let options = self
            .options(entry)?
            .unix_permissions(entry.unix_mode.unwrap_or(0o644));
        let writer = self.writer()?;
        writer
            .start_file(archive_path, options)
            .map_err(ArchiveWriteError::new)?;
        io::copy(reader, writer)?;
        Ok(())
    }

    fn add_symlink(
        &mut self,
        archive_path: &str,
        entry: &ScannedEntry,
    ) -> Result<(), ArchiveWriteError> {
        let target = entry
            .link_target
            .as_ref()
            .ok_or_else(|| ArchiveWriteError::new("symlink target is unavailable"))?
            .to_string_lossy()
            .into_owned();
        let options = self
            .options(entry)?
            .unix_permissions(entry.unix_mode.unwrap_or(0o777));
        self.writer()?
            .add_symlink(archive_path, target, options)
            .map_err(ArchiveWriteError::new)
    }

    fn finish(mut self: Box<Self>) -> Result<File, ArchiveWriteError> {
        self.writer
            .take()
            .expect("unfinished ZIP writer")
            .finish()
            .map_err(ArchiveWriteError::new)
    }
}

struct SevenZipBackend {
    writer: Option<SevenZipWriter<File>>,
}

impl SevenZipBackend {
    fn entry(&self, archive_path: &str, entry: &ScannedEntry, directory: bool) -> SevenZipEntry {
        let mut archive_entry = if directory {
            SevenZipEntry::new_directory(archive_path)
        } else {
            SevenZipEntry::new_file(archive_path)
        };
        if let Some(modified) = system_time(entry.modified_unix_nanos)
            && let Ok(value) = sevenz_rust2::NtTime::try_from(modified)
        {
            archive_entry.last_modified_date = value;
            archive_entry.has_last_modified_date = true;
        }
        if let Some(created) = system_time(entry.created_unix_nanos)
            && let Ok(value) = sevenz_rust2::NtTime::try_from(created)
        {
            archive_entry.creation_date = value;
            archive_entry.has_creation_date = true;
        }
        if let Some(attributes) = entry
            .windows_attributes
            .or_else(|| entry.unix_mode.map(|mode| mode << 16))
        {
            archive_entry.windows_attributes = attributes;
            archive_entry.has_windows_attributes = true;
        }
        archive_entry
    }

    fn writer(&mut self) -> Result<&mut SevenZipWriter<File>, ArchiveWriteError> {
        self.writer
            .as_mut()
            .ok_or_else(|| ArchiveWriteError::new("7z writer is already finished"))
    }
}

impl ArchiveWriterBackend for SevenZipBackend {
    fn add_directory(
        &mut self,
        archive_path: &str,
        entry: &ScannedEntry,
    ) -> Result<(), ArchiveWriteError> {
        let archive_entry = self.entry(archive_path, entry, true);
        self.writer()?
            .push_archive_entry::<&[u8]>(archive_entry, None)
            .map(|_| ())
            .map_err(ArchiveWriteError::new)
    }

    fn add_file(
        &mut self,
        archive_path: &str,
        entry: &ScannedEntry,
        reader: &mut dyn Read,
    ) -> Result<(), ArchiveWriteError> {
        let archive_entry = self.entry(archive_path, entry, false);
        self.writer()?
            .push_archive_entry(archive_entry, Some(reader))
            .map(|_| ())
            .map_err(ArchiveWriteError::new)
    }

    fn add_symlink(
        &mut self,
        archive_path: &str,
        entry: &ScannedEntry,
    ) -> Result<(), ArchiveWriteError> {
        let target = entry
            .link_target
            .as_ref()
            .ok_or_else(|| ArchiveWriteError::new("symlink target is unavailable"))?
            .to_string_lossy();
        let archive_entry = self.entry(archive_path, entry, false);
        self.writer()?
            .push_archive_entry(archive_entry, Some(target.as_bytes()))
            .map(|_| ())
            .map_err(ArchiveWriteError::new)
    }

    fn finish(mut self: Box<Self>) -> Result<File, ArchiveWriteError> {
        self.writer
            .take()
            .expect("unfinished 7z writer")
            .finish()
            .map_err(ArchiveWriteError::new)
    }
}

struct TarGzBackend {
    builder: Option<Builder<GzEncoder<File>>>,
}

impl ArchiveWriterBackend for TarGzBackend {
    fn add_directory(
        &mut self,
        archive_path: &str,
        entry: &ScannedEntry,
    ) -> Result<(), ArchiveWriteError> {
        append_tar_directory(
            self.builder.as_mut().expect("unfinished TAR.GZ writer"),
            archive_path,
            entry,
        )
    }

    fn add_file(
        &mut self,
        archive_path: &str,
        entry: &ScannedEntry,
        reader: &mut dyn Read,
    ) -> Result<(), ArchiveWriteError> {
        append_tar_file(
            self.builder.as_mut().expect("unfinished TAR.GZ writer"),
            archive_path,
            entry,
            reader,
        )
    }

    fn add_symlink(
        &mut self,
        archive_path: &str,
        entry: &ScannedEntry,
    ) -> Result<(), ArchiveWriteError> {
        append_tar_symlink(
            self.builder.as_mut().expect("unfinished TAR.GZ writer"),
            archive_path,
            entry,
        )
    }

    fn finish(mut self: Box<Self>) -> Result<File, ArchiveWriteError> {
        let mut builder = self.builder.take().expect("unfinished TAR.GZ writer");
        builder.finish()?;
        builder
            .into_inner()?
            .finish()
            .map_err(ArchiveWriteError::new)
    }
}

struct TarZstBackend {
    builder: Option<Builder<zstd::stream::write::Encoder<'static, File>>>,
}

impl ArchiveWriterBackend for TarZstBackend {
    fn add_directory(
        &mut self,
        archive_path: &str,
        entry: &ScannedEntry,
    ) -> Result<(), ArchiveWriteError> {
        append_tar_directory(
            self.builder.as_mut().expect("unfinished TAR.ZST writer"),
            archive_path,
            entry,
        )
    }

    fn add_file(
        &mut self,
        archive_path: &str,
        entry: &ScannedEntry,
        reader: &mut dyn Read,
    ) -> Result<(), ArchiveWriteError> {
        append_tar_file(
            self.builder.as_mut().expect("unfinished TAR.ZST writer"),
            archive_path,
            entry,
            reader,
        )
    }

    fn add_symlink(
        &mut self,
        archive_path: &str,
        entry: &ScannedEntry,
    ) -> Result<(), ArchiveWriteError> {
        append_tar_symlink(
            self.builder.as_mut().expect("unfinished TAR.ZST writer"),
            archive_path,
            entry,
        )
    }

    fn finish(mut self: Box<Self>) -> Result<File, ArchiveWriteError> {
        let mut builder = self.builder.take().expect("unfinished TAR.ZST writer");
        builder.finish()?;
        builder
            .into_inner()?
            .finish()
            .map_err(ArchiveWriteError::new)
    }
}

fn append_tar_directory<W: Write>(
    builder: &mut Builder<W>,
    archive_path: &str,
    entry: &ScannedEntry,
) -> Result<(), ArchiveWriteError> {
    let mut header = tar_header(entry, EntryType::Directory, 0, 0o755);
    header.set_cksum();
    builder
        .append_data(
            &mut header,
            ensure_directory_path(archive_path),
            io::empty(),
        )
        .map_err(Into::into)
}

fn append_tar_file<W: Write>(
    builder: &mut Builder<W>,
    archive_path: &str,
    entry: &ScannedEntry,
    reader: &mut dyn Read,
) -> Result<(), ArchiveWriteError> {
    let mut header = tar_header(entry, EntryType::Regular, entry.size, 0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, archive_path, reader)
        .map_err(Into::into)
}

fn append_tar_symlink<W: Write>(
    builder: &mut Builder<W>,
    archive_path: &str,
    entry: &ScannedEntry,
) -> Result<(), ArchiveWriteError> {
    let target = entry
        .link_target
        .as_ref()
        .ok_or_else(|| ArchiveWriteError::new("symlink target is unavailable"))?;
    let mut header = tar_header(entry, EntryType::Symlink, 0, 0o777);
    header
        .set_link_name(target)
        .map_err(ArchiveWriteError::new)?;
    header.set_cksum();
    builder
        .append_data(&mut header, archive_path, io::empty())
        .map_err(Into::into)
}

fn tar_header(entry: &ScannedEntry, kind: EntryType, size: u64, mode: u32) -> Header {
    let mut header = Header::new_gnu();
    header.set_entry_type(kind);
    header.set_size(size);
    header.set_mode(entry.unix_mode.unwrap_or(mode) & 0o7777);
    header.set_mtime(entry.modified_unix_nanos.unwrap_or(0) / 1_000_000_000);
    header
}

fn system_time(unix_nanos: Option<u64>) -> Option<std::time::SystemTime> {
    unix_nanos.map(|nanos| {
        std::time::UNIX_EPOCH
            + std::time::Duration::new(nanos / 1_000_000_000, (nanos % 1_000_000_000) as u32)
    })
}

fn ensure_directory_path(path: &str) -> String {
    if path.ends_with('/') {
        path.to_owned()
    } else {
        format!("{path}/")
    }
}
