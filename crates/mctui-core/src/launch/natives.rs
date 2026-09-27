//! Unpacking native libraries (`.dll`, `.so`, `.dylib`) from their jars
//! into the natives folder, where Java looks for them. Before 1.19 every
//! version needs this to start (the Go app never did it: L-1); from 1.19,
//! LWJGL unpacks its own.

use std::fs::{self, File};
use std::io;
use std::path::Path;

use super::plan::{NativeJar, Plan};
use super::{Error, Result, blocking};
use crate::fsutil;

/// Records what the natives folder was unpacked for.
const MARKER: &str = ".mctui-natives";

/// Makes the plan's natives folder hold its natives jars, unpacked. The
/// folder is rebuilt whole when the version or its jars changed since it was
/// last unpacked (stale natives of another LWJGL break the game), and left
/// alone otherwise, along with whatever the game unpacked there itself.
pub(super) async fn unpack(plan: &Plan) -> Result<()> {
    let key = marker(plan);
    let (dir, jars) = (plan.natives_dir.clone(), plan.natives.clone());
    blocking(move || {
        let current = fs::read_to_string(dir.join(MARKER));
        if current.is_ok_and(|current| current == key) {
            return Ok(());
        }
        rebuild(&dir, &jars, &key)
    })
    .await
}

/// What the folder is for: the version and every jar in it.
fn marker(plan: &Plan) -> String {
    let mut key = plan.version.clone();
    for native in &plan.natives {
        key.push('\n');
        key.push_str(&native.jar.to_string_lossy());
    }
    key
}

/// Unpacks `jars` into a scratch folder beside `dir`, then swaps it in, so
/// `dir` is never half-unpacked, even when the launch is cancelled. Blocking.
fn rebuild(dir: &Path, jars: &[NativeJar], key: &str) -> Result<()> {
    let parent = dir.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(Error::io(parent))?;
    let staging = tempfile::Builder::new()
        .prefix(".natives-")
        .tempdir_in(parent)
        .map_err(Error::io(parent))?;
    for jar in jars {
        extract(jar, staging.path())?;
    }
    let marker = staging.path().join(MARKER);
    fs::write(&marker, key).map_err(Error::io(marker))?;
    fsutil::replace_dir(staging.path(), dir, parent).map_err(Error::io(dir))
    // `staging` now finds nothing to delete when dropped.
}

/// Writes every file of `native.jar` into `dest` but those `exclude`d. A
/// path that would leave `dest` (or a symlink, which could point anywhere)
/// fails the whole unpacking; an absolute one is taken as relative to it.
fn extract(native: &NativeJar, dest: &Path) -> Result<()> {
    let jar = &native.jar;
    let failed = |source| Error::Natives {
        jar: jar.clone(),
        source,
    };
    let file = File::open(jar).map_err(failed)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| failed(e.into()))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| failed(e.into()))?;
        let name = entry.name().to_owned();
        if entry.is_dir() || native.exclude.iter().any(|prefix| name.starts_with(prefix)) {
            continue;
        }
        let unsafe_entry = || Error::UnsafeNatives {
            jar: jar.clone(),
            entry: name.clone(),
        };
        if entry.is_symlink() {
            return Err(unsafe_entry());
        }
        let path = dest.join(entry.enclosed_name().ok_or_else(unsafe_entry)?);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(Error::io(parent))?;
        }
        let mut out = File::create(&path).map_err(Error::io(&path))?;
        io::copy(&mut entry, &mut out).map_err(failed)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::path::PathBuf;

    use zip::write::SimpleFileOptions;

    use super::*;

    fn jar(dir: &Path, name: &str, entries: &[(&str, &str)]) -> PathBuf {
        let path = dir.join(name);
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, body) in entries {
            if let Some(dir) = name.strip_suffix('/') {
                zip.add_directory(dir, SimpleFileOptions::default())
                    .unwrap();
            } else {
                zip.start_file(*name, SimpleFileOptions::default()).unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
        }
        zip.finish().unwrap();
        path
    }

    fn native(jar: PathBuf) -> NativeJar {
        NativeJar {
            jar,
            exclude: vec!["META-INF/".into()],
        }
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = walk(dir)
            .into_iter()
            .map(|p| {
                p.strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        names.sort();
        names
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        fs::read_dir(dir)
            .unwrap()
            .flat_map(|entry| {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path)
                } else {
                    vec![path]
                }
            })
            .collect()
    }

    #[test]
    fn extracts_all_but_the_excluded() {
        let tmp = tempfile::tempdir().unwrap();
        let jar = jar(
            tmp.path(),
            "natives.jar",
            &[
                ("META-INF/", ""),
                ("META-INF/MANIFEST.MF", "manifest"),
                ("liblwjgl.so", "elf"),
                ("sub/libopenal.so", "elf too"),
            ],
        );
        let out = tmp.path().join("out");
        fs::create_dir(&out).unwrap();
        extract(&native(jar), &out).unwrap();
        assert_eq!(names(&out), ["liblwjgl.so", "sub/libopenal.so"]);
        assert_eq!(fs::read_to_string(out.join("liblwjgl.so")).unwrap(), "elf");
    }

    #[test]
    fn paths_leaving_the_folder_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        for evil in ["../evil.so", "sub/../../evil.so"] {
            let jar = jar(tmp.path(), "evil.jar", &[("ok.so", ""), (evil, "boo")]);
            let dir = tmp.path().join("natives");
            let err = rebuild(&dir, &[native(jar)], "key").unwrap_err();
            assert!(
                matches!(&err, Error::UnsafeNatives { entry, .. } if entry == evil),
                "{err}"
            );
            assert!(!dir.exists(), "nothing unpacked");
            assert!(!tmp.path().join("evil.so").exists());
        }
        let left: Vec<_> = fs::read_dir(tmp.path()).unwrap().collect();
        assert_eq!(left.len(), 1, "only the jar; no scratch folders");
    }

    #[tokio::test]
    async fn unpacks_once_per_version_and_jar_set() {
        let tmp = tempfile::tempdir().unwrap();
        let a = jar(tmp.path(), "a.jar", &[("a.so", "a")]);
        let b = jar(tmp.path(), "b.jar", &[("b.so", "b")]);
        let instance = tmp.path().join("instance");
        let mut plan = test_plan(&instance, "1.8.9", vec![native(a.clone())]);

        unpack(&plan).await.unwrap();
        assert_eq!(names(&plan.natives_dir), [".mctui-natives", "a.so"]);

        // Unchanged: left alone, with what the game added.
        fs::write(plan.natives_dir.join("extracted-by-game.so"), "").unwrap();
        unpack(&plan).await.unwrap();
        assert!(plan.natives_dir.join("extracted-by-game.so").exists());

        // Other jars: rebuilt from scratch.
        plan.natives = vec![native(a), native(b)];
        unpack(&plan).await.unwrap();
        assert_eq!(names(&plan.natives_dir), [".mctui-natives", "a.so", "b.so"]);

        // Another version with no natives jars: emptied, stale LWJGL gone.
        let modern = test_plan(&instance, "1.21.8", Vec::new());
        unpack(&modern).await.unwrap();
        assert_eq!(names(&modern.natives_dir), [".mctui-natives"]);
        let mut left: Vec<_> = fs::read_dir(&instance)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        left.sort();
        assert_eq!(left, ["natives"], "no scratch folders");
    }

    fn test_plan(instance: &Path, version: &str, natives: Vec<NativeJar>) -> Plan {
        let mut plan = super::super::plan::tests::empty_plan(instance);
        plan.version = version.to_owned();
        plan.natives = natives;
        plan
    }
}
