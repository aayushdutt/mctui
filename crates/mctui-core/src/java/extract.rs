//! Unpacking a runtime archive (`.tar.gz` or `.zip`) without letting it
//! reach outside its folder, keeping Unix permissions (the exec bits).

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Seek};
use std::path::{Component, Path, PathBuf};

use super::Error;

/// Unpacks `archive` into `dest`, dropping the one top-level folder Adoptium
/// wraps everything in (`jdk-21.0.4+7-jre/`). The format comes from the file
/// name. Blocking.
///
/// Every entry must name a path inside `dest`, and every symlink must point
/// inside it. Symlinks are created last, so nothing is written through one.
pub(super) fn unpack(archive: &Path, dest: &Path) -> Result<(), Error> {
    let extension = archive
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase);
    let file = BufReader::new(File::open(archive)?);
    let mut out = Unpacker {
        dest,
        symlinks: Vec::new(),
    };
    match extension.as_deref() {
        Some("gz" | "tgz") => unpack_tar_gz(file, &mut out)?,
        Some("zip") => unpack_zip(file, &mut out)?,
        _ => {
            let message = format!("{} isn't a .tar.gz or .zip", archive.display());
            return Err(io::Error::new(io::ErrorKind::InvalidInput, message).into());
        }
    }
    out.finish()
}

fn unpack_tar_gz(file: impl Read, out: &mut Unpacker) -> Result<(), Error> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            out.dir(&path)?;
        } else if kind.is_file() {
            let mode = entry.header().mode().ok();
            out.file(&path, &mut entry, mode)?;
        } else if kind.is_symlink() {
            let link = entry.link_name()?.unwrap_or_default().into_owned();
            out.symlink(&path, &link)?;
        }
        // Other kinds (hard links, devices) don't occur in runtimes.
    }
    Ok(())
}

fn unpack_zip(file: impl Read + Seek, out: &mut Unpacker) -> Result<(), Error> {
    let mut archive = zip::ZipArchive::new(file).map_err(io::Error::from)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(io::Error::from)?;
        let path = PathBuf::from(entry.name());
        if entry.is_dir() {
            out.dir(&path)?;
        } else if entry.is_symlink() {
            let mut link = String::new();
            entry.read_to_string(&mut link)?;
            out.symlink(&path, Path::new(&link))?;
        } else {
            let mode = entry.unix_mode();
            out.file(&path, &mut entry, mode)?;
        }
    }
    Ok(())
}

/// Writes archive entries under `dest`, minus their top-level folder.
struct Unpacker<'a> {
    dest: &'a Path,
    /// `(where, pointing to)`, `where` relative to `dest`; created by
    /// [`Unpacker::finish`].
    symlinks: Vec<(PathBuf, PathBuf)>,
}

impl Unpacker<'_> {
    fn dir(&self, entry: &Path) -> Result<(), Error> {
        if let Some(rel) = relative(entry)? {
            fs::create_dir_all(self.dest.join(rel))?;
        }
        Ok(())
    }

    fn file(&self, entry: &Path, contents: &mut impl Read, mode: Option<u32>) -> Result<(), Error> {
        let Some(rel) = relative(entry)? else {
            return Ok(());
        };
        let target = self.dest.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        io::copy(contents, &mut create(&target, mode)?)?;
        Ok(())
    }

    fn symlink(&mut self, entry: &Path, link: &Path) -> Result<(), Error> {
        if let Some(rel) = relative(entry)? {
            self.symlinks.push((rel, link.to_owned()));
        }
        Ok(())
    }

    /// Creates the symlinks, once every one is known. Each must point inside
    /// `dest`, judged literally. That's exact because no link may sit under
    /// another or point through one: a runtime's links point straight at
    /// files and folders.
    fn finish(self) -> Result<(), Error> {
        let links: HashSet<&Path> = self.symlinks.iter().map(|(rel, _)| rel.as_path()).collect();
        for (rel, link) in &self.symlinks {
            let dir = rel.parent().unwrap_or(Path::new(""));
            let under_a_link = dir.ancestors().any(|folder| links.contains(folder));
            if under_a_link || link.as_os_str().is_empty() || follow(dir, link, &links).is_none() {
                return Err(Error::UnsafeArchive { entry: rel.clone() });
            }
        }
        for (rel, link) in &self.symlinks {
            let path = self.dest.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            symlink(link, &path)?;
        }
        Ok(())
    }
}

/// `entry` without its top-level folder, as a path under `dest`; `None` for
/// the top-level folder itself. Errors if it would leave `dest`.
fn relative(entry: &Path) -> Result<Option<PathBuf>, Error> {
    let unsafe_entry = || Error::UnsafeArchive {
        entry: entry.to_owned(),
    };
    let mut parts = entry.components().filter(|c| *c != Component::CurDir);
    if !matches!(parts.next(), Some(Component::Normal(_))) {
        return Err(unsafe_entry());
    }
    let rest = parts
        .map(|part| match part {
            Component::Normal(name) => Ok(name),
            _ => Err(unsafe_entry()),
        })
        .collect::<Result<PathBuf, _>>()?;
    Ok((!rest.as_os_str().is_empty()).then_some(rest))
}

/// Where `link`, followed from `dir`, points (both relative to the root).
/// `None` if it leaves the root or goes on through another of the `links`.
fn follow(dir: &Path, link: &Path, links: &HashSet<&Path>) -> Option<PathBuf> {
    let mut at = dir.to_owned();
    for part in link.components() {
        if links.contains(at.as_path()) {
            return None;
        }
        match part {
            Component::Normal(name) => at.push(name),
            Component::CurDir => {}
            Component::ParentDir => {
                if !at.pop() {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(at)
}

/// Creates (or truncates) a file with the archive's permission bits, less
/// setuid/setgid/sticky and group/other write.
fn create(path: &Path, mode: Option<u32>) -> io::Result<File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    if let Some(mode) = mode {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(mode & 0o755);
    }
    #[cfg(not(unix))]
    let _ = mode;
    options.open(path)
}

#[cfg(unix)]
fn symlink(link: &Path, path: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(link, path)
}

/// Runtimes for Windows ship as zips without symlinks.
#[cfg(not(unix))]
fn symlink(_link: &Path, path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        format!("can't create the symlink {}", path.display()),
    ))
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    /// An archive entry for the test builders.
    enum Entry<'a> {
        Dir(&'a str),
        File(&'a str, &'a [u8], u32),
        Link(&'a str, &'a str),
    }
    use Entry::{Dir, File as F, Link};

    fn tar_gz(entries: &[Entry]) -> Vec<u8> {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut tar = tar::Builder::new(gz);
        for entry in entries {
            let mut header = tar::Header::new_gnu();
            let (name, data, kind, mode): (&str, &[u8], _, _) = match *entry {
                Dir(name) => (name, b"", tar::EntryType::Directory, 0o755),
                F(name, data, mode) => (name, data, tar::EntryType::Regular, mode),
                Link(name, target) => {
                    header.set_link_name(target).unwrap();
                    (name, b"", tar::EntryType::Symlink, 0o777)
                }
            };
            // Written raw: `set_path` refuses the `..` the tests need.
            header.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
            header.set_entry_type(kind);
            header.set_mode(mode);
            header.set_size(data.len() as u64);
            header.set_cksum();
            tar.append(&header, data).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap()
    }

    fn zip(entries: &[Entry]) -> Vec<u8> {
        use zip::write::SimpleFileOptions;
        let mut zip = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for entry in entries {
            match *entry {
                Dir(name) => zip.add_directory(name, options).unwrap(),
                F(name, data, mode) => {
                    zip.start_file(name, options.unix_permissions(mode))
                        .unwrap();
                    zip.write_all(data).unwrap();
                }
                Link(name, target) => zip.add_symlink(name, target, options).unwrap(),
            }
        }
        zip.finish().unwrap().into_inner()
    }

    /// Unpacks `bytes`, saved as `name`, into `<tmp>/out`.
    fn unpack_bytes(name: &str, bytes: &[u8]) -> (tempfile::TempDir, PathBuf, Result<(), Error>) {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join(name);
        fs::write(&archive, bytes).unwrap();
        let out = tmp.path().join("out");
        fs::create_dir(&out).unwrap();
        let result = unpack(&archive, &out);
        (tmp, out, result)
    }

    const RUNTIME: &[Entry] = &[
        Dir("jdk-21+7-jre/"),
        F("jdk-21+7-jre/bin/java", b"#!/bin/sh\n", 0o755),
        F("jdk-21+7-jre/release", b"JAVA_VERSION=\"21.0.4\"\n", 0o644),
        F("jdk-21+7-jre/legal/java.base/LICENSE", b"GPL", 0o644),
        Link(
            "jdk-21+7-jre/legal/java.xml/LICENSE",
            "../java.base/LICENSE",
        ),
    ];

    fn assert_unpacked(out: &Path) {
        assert_eq!(fs::read(out.join("bin/java")).unwrap(), b"#!/bin/sh\n");
        assert!(out.join("release").is_file());
        assert!(
            !out.join("jdk-21+7-jre").exists(),
            "top-level folder dropped"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = |p: &str| fs::metadata(out.join(p)).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode("bin/java") & 0o111, 0o111, "executable");
            assert_eq!(mode("release") & 0o111, 0, "not executable");
            let link = out.join("legal/java.xml/LICENSE");
            assert_eq!(
                fs::read_link(&link).unwrap(),
                Path::new("../java.base/LICENSE")
            );
            assert_eq!(fs::read(link).unwrap(), b"GPL");
        }
    }

    #[cfg(unix)]
    #[test]
    fn unpacks_tar_gz() {
        let (_tmp, out, result) = unpack_bytes("jre.tar.gz", &tar_gz(RUNTIME));
        result.unwrap();
        assert_unpacked(&out);
    }

    #[cfg(unix)]
    #[test]
    fn unpacks_zip() {
        let (_tmp, out, result) = unpack_bytes("jre.zip", &zip(RUNTIME));
        result.unwrap();
        assert_unpacked(&out);
    }

    #[test]
    fn rejects_entries_escaping_the_folder() {
        let escapes: &[&[Entry]] = &[
            &[F("jdk/../../evil", b"x", 0o644)],
            &[F("../evil", b"x", 0o644)],
            &[F("/tmp/evil", b"x", 0o644)],
            &[Dir("jdk/../../evil/")],
            &[Link("jdk/lib/evil", "../../../outside")],
            &[Link("jdk/evil", "/etc/passwd")],
            // Each link alone looks inside, but `s` really points above the
            // root (through `a/b`), and `s/evil` would be made up there.
            &[
                Link("jdk/a/b", ".."),
                Link("jdk/s", "a/b/.."),
                Link("jdk/s/evil", "x"),
            ],
        ];
        for (i, entries) in escapes.iter().enumerate() {
            for (name, bytes) in [("a.tar.gz", tar_gz(entries)), ("a.zip", zip(entries))] {
                let (tmp, out, result) = unpack_bytes(name, &bytes);
                assert!(
                    matches!(result, Err(Error::UnsafeArchive { .. })),
                    "case {i} in {name}: {result:?}"
                );
                // Not even a dangling link.
                assert!(fs::symlink_metadata(tmp.path().join("evil")).is_err());
                assert_eq!(fs::read_dir(&out).unwrap().count(), 0, "case {i} in {name}");
            }
        }
    }

    #[test]
    fn links_are_followed_from_their_own_folder() {
        let links = HashSet::from([Path::new("lib/current")]);
        for (dir, link, to) in [
            ("", "a", Some("a")),
            ("", "..", None),
            (
                "legal/java.xml",
                "../java.base/LICENSE",
                Some("legal/java.base/LICENSE"),
            ),
            ("legal/java.xml", "../../release", Some("release")),
            ("legal/java.xml", "../../../x", None),
            ("bin", "./../lib/./libjli.so", Some("lib/libjli.so")),
            ("bin", "/usr/lib/libjli.so", None),
            // Pointing at another link is fine; going through one isn't.
            ("bin", "../lib/current", Some("lib/current")),
            ("bin", "../lib/current/libjli.so", None),
        ] {
            let followed = follow(Path::new(dir), Path::new(link), &links);
            assert_eq!(followed, to.map(PathBuf::from), "{dir} -> {link}");
        }
    }

    #[test]
    fn rejects_unknown_formats() {
        let (_tmp, _out, result) = unpack_bytes("jre.pkg", b"xar!");
        assert!(matches!(result, Err(Error::Io(_))), "{result:?}");
    }
}
