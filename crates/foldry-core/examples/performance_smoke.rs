use std::{
    fs::{self, File},
    hint::black_box,
    io::{self, Cursor, Read},
    time::Instant,
};

use foldry_core::{
    ArchiveFormat, CompiledProfile, CompressionLevel, FileSystemCaseSensitivity,
    FileSystemObjectKind, FileSystemScanner, ProfileId, ScanDisposition, ScanNotice, ScanSink,
    ScanSinkError, ScannedEntry, create_archive_writer, parse_profile,
};
use serde_json::json;

const DEFAULT_SMALL_FILES: usize = 5_000;
const DEFAULT_MATCHES: usize = 1_000_000;
const DEFAULT_LARGE_BYTES: u64 = 64 * 1024 * 1024;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let small_files = env_usize("FOLDRY_BENCH_SMALL_FILES", DEFAULT_SMALL_FILES);
    let match_iterations = env_usize("FOLDRY_BENCH_MATCHES", DEFAULT_MATCHES);
    let large_bytes = env_u64("FOLDRY_BENCH_LARGE_BYTES", DEFAULT_LARGE_BYTES);
    let profile_id = ProfileId::new();
    let profile = parse_profile(&format!(
        "# @profile-id {profile_id}\n# @profile-version 1\n# @profile-name Benchmark\n\
         target/\n*.tmp\n!important.tmp\n"
    ))
    .profile
    .ok_or("benchmark profile did not parse")?;
    let matcher = CompiledProfile::new(&profile, FileSystemCaseSensitivity::Sensitive)?;

    let matcher_started = Instant::now();
    for index in 0..match_iterations {
        let path = if index % 16 == 0 {
            format!("target/file-{index}.bin")
        } else if index % 31 == 0 {
            format!("src/file-{index}.tmp")
        } else {
            format!("src/module-{}/file-{index}.rs", index % 128)
        };
        black_box(matcher.matched(&path, false)?);
    }
    let matcher_elapsed = matcher_started.elapsed();

    let source = tempfile::tempdir()?;
    for index in 0..small_files {
        let directory = source.path().join(format!("group-{:03}", index % 100));
        fs::create_dir_all(&directory)?;
        fs::write(
            directory.join(format!("file-{index:06}.txt")),
            format!("Foldry benchmark payload {index:06}\n"),
        )?;
    }
    let scanner_started = Instant::now();
    let mut sink = CountingSink::default();
    let scanner_summary =
        FileSystemScanner::scan(source.path(), &matcher, &mut sink, &Default::default())?;
    let scanner_elapsed = scanner_started.elapsed();

    let archive_directory = tempfile::tempdir()?;
    let mut writer_results = Vec::new();
    for format in [
        ArchiveFormat::Zip,
        ArchiveFormat::TarGz,
        ArchiveFormat::TarZst,
    ] {
        let path = archive_directory.path().join(format_name(format));
        let started = Instant::now();
        let mut writer =
            create_archive_writer(format, CompressionLevel::Fast, File::create(&path)?)?;
        for index in 0..small_files {
            let entry = synthetic_entry(128);
            let mut reader = Cursor::new([index as u8; 128]);
            writer.add_file(&format!("small/file-{index:06}.bin"), &entry, &mut reader)?;
        }
        let large_entry = synthetic_entry(large_bytes);
        let mut large_reader = PatternReader::new(large_bytes);
        writer.add_file("large/payload.bin", &large_entry, &mut large_reader)?;
        writer.finish()?.sync_all()?;
        let elapsed = started.elapsed();
        writer_results.push(json!({
            "format": format_name(format),
            "elapsed_ms": elapsed.as_millis(),
            "input_bytes": large_bytes + (small_files as u64 * 128),
            "output_bytes": fs::metadata(path)?.len()
        }));
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "workload": {
                "small_files": small_files,
                "matcher_iterations": match_iterations,
                "large_bytes": large_bytes
            },
            "matcher": {
                "elapsed_ms": matcher_elapsed.as_millis(),
                "operations_per_second": rate(match_iterations as u64, matcher_elapsed.as_secs_f64())
            },
            "scanner": {
                "elapsed_ms": scanner_elapsed.as_millis(),
                "entries": scanner_summary.visited_entries,
                "entries_per_second": rate(scanner_summary.visited_entries, scanner_elapsed.as_secs_f64()),
                "sink_entries": sink.entries
            },
            "writers": writer_results
        }))?
    );
    Ok(())
}

#[derive(Default)]
struct CountingSink {
    entries: u64,
}

impl ScanSink for CountingSink {
    fn write_entry(&mut self, _entry: &ScannedEntry) -> Result<(), ScanSinkError> {
        self.entries += 1;
        Ok(())
    }

    fn write_notice(&mut self, _notice: &ScanNotice) -> Result<(), ScanSinkError> {
        Ok(())
    }
}

fn synthetic_entry(size: u64) -> ScannedEntry {
    ScannedEntry {
        relative_path: "synthetic".into(),
        native_path: "synthetic".into(),
        kind: FileSystemObjectKind::RegularFile,
        disposition: ScanDisposition::Included,
        size,
        modified_unix_nanos: None,
        link_target: None,
        is_mount_point: false,
        is_network_mount: false,
        reason: None,
    }
}

struct PatternReader {
    remaining: u64,
    position: u8,
}

impl PatternReader {
    const fn new(remaining: u64) -> Self {
        Self {
            remaining,
            position: 0,
        }
    }
}

impl Read for PatternReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count =
            usize::try_from(self.remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        for byte in &mut buffer[..count] {
            *byte = self.position;
            self.position = self.position.wrapping_add(1);
        }
        self.remaining -= count as u64;
        Ok(count)
    }
}

fn rate(count: u64, seconds: f64) -> u64 {
    if seconds <= f64::EPSILON {
        count
    } else {
        (count as f64 / seconds).round() as u64
    }
}

const fn format_name(format: ArchiveFormat) -> &'static str {
    match format {
        ArchiveFormat::Zip => "zip",
        ArchiveFormat::TarGz => "tar_gz",
        ArchiveFormat::TarZst => "tar_zst",
    }
}

fn env_usize(name: &str, fallback: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn env_u64(name: &str, fallback: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}
