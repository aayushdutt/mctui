//! The player's own JVM arguments: the app-wide ones from Settings, with an
//! instance's on top.

/// The garbage collectors a `-XX:+Use…GC` flag picks. Java refuses to start
/// with two picked.
const COLLECTORS: [&str; 9] = [
    "UseSerialGC",
    "UseParallelGC",
    "UseParallelOldGC",
    "UseParNewGC",
    "UseConcMarkSweepGC",
    "UseG1GC",
    "UseZGC",
    "UseShenandoahGC",
    "UseEpsilonGC",
];

/// The app-wide `global` arguments, minus any flag `instance` sets too, then
/// `instance`'s. So an instance's memory setting (`-Xmx6G`) replaces the
/// app-wide `-Xmx`, and its garbage collector (`-XX:+UseZGC`) the app-wide
/// one, and every other app-wide argument is kept (the Go app dropped them
/// all: L-10). An initial heap (`-Xms`) left larger than the maximum is
/// dropped, since Java refuses to start with it.
pub(super) fn merge(global: &[String], instance: &[String]) -> Vec<String> {
    let instance_picks_collector = instance.iter().any(|arg| picks_collector(arg));
    let overridden = |arg: &String| {
        (instance_picks_collector && picks_collector(arg))
            || flag(arg).is_some_and(|sets| instance.iter().any(|mine| flag(mine) == Some(sets)))
    };
    let mut args: Vec<String> = global
        .iter()
        .filter(|arg| !overridden(arg))
        .chain(instance)
        .cloned()
        .collect();
    drop_initial_heap_over_max(&mut args);
    args
}

/// What an argument sets, for arguments where a later one replaces an
/// earlier one. Others (`-javaagent:…`, `--add-opens` and its value) may
/// rightly repeat, so they're all kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flag<'a> {
    /// `-Xmx2G`, `-Xms`, `-Xss`, `-Xmn`: a size.
    Size(&'static str),
    /// `-XX:+UseZGC`, `-XX:-UseZGC`, `-XX:MaxGCPauseMillis=50`.
    Option(&'a str),
    /// `-Dfile.encoding=UTF-8`.
    Property(&'a str),
}

fn flag(arg: &str) -> Option<Flag<'_>> {
    const SIZES: [&str; 4] = ["-Xmx", "-Xms", "-Xss", "-Xmn"];
    if let Some(option) = arg.strip_prefix("-XX:") {
        let option = option.trim_start_matches(['+', '-']);
        return Some(Flag::Option(
            option.split_once('=').map_or(option, |(name, _)| name),
        ));
    }
    if let Some(property) = arg.strip_prefix("-D") {
        return Some(Flag::Property(
            property.split_once('=').map_or(property, |(name, _)| name),
        ));
    }
    SIZES
        .into_iter()
        .find(|size| arg.starts_with(size))
        .map(Flag::Size)
}

/// Whether `arg` turns a garbage collector on: `-XX:+UseG1GC`.
fn picks_collector(arg: &str) -> bool {
    arg.strip_prefix("-XX:+")
        .is_some_and(|option| COLLECTORS.contains(&option))
}

fn drop_initial_heap_over_max(args: &mut Vec<String>) {
    // Java uses the last of each.
    let last = |flag: &str| args.iter().rev().find_map(|arg| heap_bytes(arg, flag));
    if let (Some(initial), Some(max)) = (last("-Xms"), last("-Xmx"))
        && initial > max
    {
        tracing::warn!(
            initial,
            max,
            "dropping an initial heap size above the maximum"
        );
        args.retain(|arg| heap_bytes(arg, "-Xms").is_none());
    }
}

/// The size `arg` gives `flag` (`-Xmx2G` is 2 GiB), if it's that flag with a
/// size Java would accept.
fn heap_bytes(arg: &str, flag: &str) -> Option<u64> {
    let value = arg.strip_prefix(flag)?;
    let digits = value
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(value.len());
    let (number, unit) = value.split_at(digits);
    let scale: u64 = match unit {
        "" => 1,
        "k" | "K" => 1 << 10,
        "m" | "M" => 1 << 20,
        "g" | "G" => 1 << 30,
        "t" | "T" => 1 << 40,
        _ => return None,
    };
    number.parse::<u64>().ok()?.checked_mul(scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|&a| a.to_owned()).collect()
    }

    /// What's tested, the app's arguments, the instance's, and the result.
    type Case = (
        &'static str,
        &'static [&'static str],
        &'static [&'static str],
        &'static [&'static str],
    );

    #[test]
    fn instance_flags_replace_the_same_global_flag() {
        let cases: &[Case] = &[
            (
                "memory replaces memory only",
                &["-Xmx2G", "-Xms512M", "-XX:+UseG1GC"],
                &["-Xmx6G"],
                &["-Xms512M", "-XX:+UseG1GC", "-Xmx6G"],
            ),
            (
                "no instance args",
                &["-Xmx2G", "-Xms512M"],
                &[],
                &["-Xmx2G", "-Xms512M"],
            ),
            ("no global args", &[], &["-Xmx4G"], &["-Xmx4G"]),
            (
                "-XX options by name, whatever the sign or value",
                &[
                    "-XX:+UseG1GC",
                    "-XX:MaxGCPauseMillis=50",
                    "-XX:+AlwaysPreTouch",
                ],
                &["-XX:-UseG1GC", "-XX:MaxGCPauseMillis=20"],
                &[
                    "-XX:+AlwaysPreTouch",
                    "-XX:-UseG1GC",
                    "-XX:MaxGCPauseMillis=20",
                ],
            ),
            (
                "properties by name",
                &["-Dfile.encoding=UTF-8", "-Dfoo=1", "-Dflag"],
                &["-Dfoo=2", "-Dflag=on"],
                &["-Dfile.encoding=UTF-8", "-Dfoo=2", "-Dflag=on"],
            ),
            (
                "other arguments are all kept",
                &[
                    "-javaagent:a.jar",
                    "--add-opens",
                    "java.base/java.lang=ALL-UNNAMED",
                ],
                &["-javaagent:b.jar", "-Xlog:gc"],
                &[
                    "-javaagent:a.jar",
                    "--add-opens",
                    "java.base/java.lang=ALL-UNNAMED",
                    "-javaagent:b.jar",
                    "-Xlog:gc",
                ],
            ),
            (
                "the instance's garbage collector replaces the app's",
                &["-Xmx2G", "-XX:+UseG1GC", "-XX:MaxGCPauseMillis=50"],
                &["-XX:+UseZGC"],
                &["-Xmx2G", "-XX:MaxGCPauseMillis=50", "-XX:+UseZGC"],
            ),
            (
                "whichever collector each picks",
                &["-XX:+UseZGC", "-XX:+ZGenerational", "-XX:+UseShenandoahGC"],
                &["-XX:+UseG1GC"],
                &["-XX:+ZGenerational", "-XX:+UseG1GC"],
            ),
            (
                "options that merely end in GC aren't collectors",
                &["-XX:+UseMaximumCompactionOnSystemGC", "-XX:+UseG1GC"],
                &["-XX:+DisableExplicitGC"],
                &[
                    "-XX:+UseMaximumCompactionOnSystemGC",
                    "-XX:+UseG1GC",
                    "-XX:+DisableExplicitGC",
                ],
            ),
            (
                "an initial heap over the maximum is dropped",
                &["-Xmx2G", "-Xms1G"],
                &["-Xmx512M"],
                &["-Xmx512M"],
            ),
            (
                "an initial heap equal to the maximum is fine",
                &["-Xms1024m"],
                &["-Xmx1G"],
                &["-Xms1024m", "-Xmx1G"],
            ),
        ];
        for (name, global, instance, want) in cases {
            assert_eq!(merge(&strings(global), &strings(instance)), *want, "{name}");
        }
    }

    #[test]
    fn heap_sizes() {
        for (arg, want) in [
            ("-Xmx2G", Some(2 << 30)),
            ("-Xmx512m", Some(512 << 20)),
            ("-Xmx1048576", Some(1 << 20)),
            ("-Xmx64k", Some(64 << 10)),
            ("-Xmx2GB", None),
            ("-Xmx", None),
            ("-Xms2G", None),
        ] {
            assert_eq!(heap_bytes(arg, "-Xmx"), want, "{arg}");
        }
    }
}
