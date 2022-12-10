use anyhow::{bail, Result};
use bzip2::read::BzEncoder;
use bzip2::Compression;
use clap::{ArgAction, Parser, ValueEnum};
use md5::Digest;

use serde_json::json;
use std::collections::HashMap;
use std::fmt::Display;
use std::fs::File;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use std::{env, fs};
use tar::{Header, EntryType};

/// Simple program to greet a person
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Files to add to archive
    #[arg(long, short, action = ArgAction::Set, num_args = 1..)]
    input: Vec<PathBuf>,
    /// Path to archive. If omitted "archive.tar.<compression>" is created in current working directory
    #[arg(long, short)]
    output: Option<PathBuf>,
    /// Compression algorithm
    #[arg(long, short, value_enum, default_value_t = Comp::BZ2)]
    compression: Comp,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum Comp {
    BZ2,
}

impl Display for Comp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let repr = match self {
            Comp::BZ2 => "bz2",
        };
        write!(f, "{}", repr)
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut output = cli
        .output
        .or_else(|| env::current_dir().ok())
        .expect("Unable to read current directory");

    output = sanitize_path(output, cli.compression);
    let all_files = resolve_input(cli.input)?;
    let mut hashes: HashMap<String, String> = HashMap::new();
    
    let tar_bz2 = File::create(output)?;
    let enc = BzEncoder::new(tar_bz2, Compression::best());
    let mut tar = tar::Builder::new(enc);
    
    for file_path in all_files {
        let hash = calculate_md5(&file_path)?;
        let mut file = File::open(&file_path)?;
        tar.append_file(&file_path, &mut file)?;
        hashes.insert(
            file_path.as_os_str().to_string_lossy().into(),
            format!("{:x}", hash),
        );
    }
    let meta = json!({
        "timestamp": current_time(),
        "checksums": hashes
    });
    let data = serde_json::to_vec(&meta)?;
    tar.append(
        &create_header("meta.json", data.len() as u64)?,
        data.as_slice(),
    )?;
    tar.finish()?;
    Ok(())
}

fn create_header<P: AsRef<Path>>(path: P, size: u64) -> Result<Header> {
    let mut header = Header::new_gnu();
    header.set_path(path)?;
    header.set_device_major(0)?;
    header.set_device_minor(0)?;
    header.set_size(size);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mode(0o644);
    header.set_entry_type(EntryType::file());
    header.set_mtime(current_time());
    header.set_cksum();
    Ok(header)
}

fn current_time() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|n| n.as_secs())
        .expect("System time before EPOCH!")
}

fn read_dir(dir: PathBuf, entries: &mut Vec<PathBuf>) -> Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                read_dir(path, entries)?;
            } else {
                entries.push(path);
            }
        }
    }
    Ok(())
}

fn resolve_input(paths: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    let mut entries: Vec<PathBuf> = vec![];
    for path in paths {
        if !path.exists() {
            bail!(
                "File '{}' does not exist!",
                path.as_os_str().to_string_lossy()
            );
        }
        if path.is_file() {
            entries.push(path);
            continue;
        }
        if path.is_dir() {
            read_dir(path, &mut entries)?
        }
    }
    Ok(entries)
}

fn sanitize_path(mut path: PathBuf, compression: Comp) -> PathBuf {
    if !path.is_dir() {
        if path.extension().is_some() {
            let filename = path.file_name().unwrap().to_string_lossy();
            let (stem, _extension) = filename.split_once('.').unwrap();
            path = path.with_file_name(stem);
        }
    } else {
        path.push("out")
    }
    path.with_extension(format!("tar.{}", compression))
}

fn calculate_md5<P: AsRef<Path>>(file: P) -> Result<Digest> {
    let mut file = File::open(file)?;
    let mut ctx = md5::Context::new();
    let mut buf = [0; 4194304];
    let mut n = file.read(&mut buf[..])?;
    while n != 0 {
        ctx.consume(&buf[..n]);
        n = file.read(&mut buf[..])?;
    }
    Ok(ctx.compute())
}
