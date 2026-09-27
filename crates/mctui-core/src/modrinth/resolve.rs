//! Which jars an install needs: the requested mod and, breadth-first, every
//! required dependency that isn't installed yet.
//!
//! The walk does no I/O. It runs over the answers fetched so far and returns
//! the lookups it still needs; the caller fetches those and walks again. Each
//! level of the dependency graph is fetched as one batch that way, and the
//! walk is testable with plain data.

use std::collections::{HashMap, HashSet, VecDeque};

use super::model::{Dependency, DependencyType, Version, VersionFile};

/// A question the walk needs answered.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum Lookup {
    /// The version of this project to install.
    Project(String),
    /// This exact version.
    Version(String),
}

/// The answers fetched so far. `None`: there's no such version, or the
/// project has none that fits.
pub(super) type Answers = HashMap<Lookup, Option<Version>>;

/// The Minecraft version and loader mods must support.
#[derive(Debug, Clone, Copy)]
pub(super) struct Target<'a> {
    pub game_version: &'a str,
    pub loader: &'a str,
}

impl Target<'_> {
    /// The jar to install from `version`, if the version fits.
    pub fn jar<'v>(&self, version: &'v Version) -> Option<&'v VersionFile> {
        version
            .supports(self.game_version, self.loader)
            .then(|| version.jar())
            .flatten()
    }
}

/// A version to install and its jar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Pick {
    pub version: Version,
    pub jar: VersionFile,
}

/// What an install will do.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Plan {
    /// The requested mod first, then its dependencies breadth-first.
    pub picks: Vec<Pick>,
    pub skipped: Vec<SkippedDep>,
    pub unresolved: Vec<UnresolvedDep>,
}

/// A dependency that was deliberately not installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedDep {
    pub project_id: Option<String>,
    /// A name for people: the project's title when known, else an id.
    pub name: String,
    pub reason: SkipReason,
}

/// Why a dependency wasn't installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The mod works without it.
    Optional,
    /// It's bundled inside the mod's jar.
    Embedded,
    /// The mod doesn't work alongside it. `installed`: it is installed, a
    /// conflict to warn about.
    Incompatible { installed: bool },
    /// It's required, and already installed.
    AlreadyInstalled,
}

/// A required dependency that couldn't be installed: the mod may crash the
/// game without it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedDep {
    pub project_id: Option<String>,
    /// A name for people: the project's title when known, else an id or the
    /// file name the mod asked for.
    pub name: String,
    pub reason: UnresolvedReason,
}

/// Why a required dependency couldn't be installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnresolvedReason {
    /// It has no version for this Minecraft version and loader (or it's gone).
    NoCompatibleVersion,
    /// It isn't on Modrinth: the mod names a file from elsewhere.
    NotOnModrinth,
}

/// Plans installing `root`, skipping the `installed` project ids, or returns
/// the lookups still needed to finish the plan.
pub(super) fn resolve(
    root: &Pick,
    target: Target<'_>,
    answers: &Answers,
    installed: &HashSet<String>,
) -> Result<Plan, Vec<Lookup>> {
    let mut plan = Plan::default();
    let mut missing = Vec::new();
    let mut seen = HashSet::from([root.version.project_id.as_str()]);
    let mut queue = VecDeque::from([(&root.version, &root.jar)]);

    while let Some((version, jar)) = queue.pop_front() {
        plan.picks.push(Pick {
            version: version.clone(),
            jar: jar.clone(),
        });
        for dep in &version.dependencies {
            let reason = match dep.dependency_type {
                DependencyType::Required => None,
                DependencyType::Optional => Some(SkipReason::Optional),
                DependencyType::Embedded => Some(SkipReason::Embedded),
                DependencyType::Incompatible => Some(SkipReason::Incompatible { installed: false }),
                DependencyType::Other => continue,
            };
            if let Some(reason) = reason {
                plan.skipped.push(SkippedDep {
                    project_id: dep.project_id.clone(),
                    name: dep_name(dep),
                    reason,
                });
                continue;
            }
            // A dependency that only pins a version names its project once
            // the version is known.
            if let Some(id) = &dep.project_id
                && !claim(id, &mut seen, installed, &mut plan.skipped)
            {
                continue;
            }
            match find(dep, target, answers) {
                Found::Pick(version, jar) => {
                    if dep.project_id.is_some()
                        || claim(&version.project_id, &mut seen, installed, &mut plan.skipped)
                    {
                        queue.push_back((version, jar));
                    }
                }
                Found::Need(lookup) => {
                    if !missing.contains(&lookup) {
                        missing.push(lookup);
                    }
                }
                Found::Unresolved(project_id, reason) => plan.unresolved.push(UnresolvedDep {
                    project_id: project_id.cloned(),
                    name: project_id.cloned().unwrap_or_else(|| dep_name(dep)),
                    reason,
                }),
            }
        }
    }
    if !missing.is_empty() {
        return Err(missing);
    }
    plan.tidy(installed);
    Ok(plan)
}

/// Marks the project `id` seen. True if it's new and needs installing; an
/// installed one is recorded as skipped.
fn claim<'a>(
    id: &'a str,
    seen: &mut HashSet<&'a str>,
    installed: &HashSet<String>,
    skipped: &mut Vec<SkippedDep>,
) -> bool {
    if !seen.insert(id) {
        return false;
    }
    if installed.contains(id) {
        skipped.push(SkippedDep {
            project_id: Some(id.to_owned()),
            name: id.to_owned(),
            reason: SkipReason::AlreadyInstalled,
        });
        return false;
    }
    true
}

impl Plan {
    /// Settles what only the whole walk knows: a dependency skipped as
    /// optional but required elsewhere is installed after all, and an
    /// incompatible one conflicts if it's installed or being installed. Drops
    /// duplicates.
    fn tidy(&mut self, installed: &HashSet<String>) {
        let picked: HashSet<&str> = self
            .picks
            .iter()
            .map(|p| p.version.project_id.as_str())
            .collect();
        let present = |id: &Option<String>| {
            id.as_deref()
                .is_some_and(|id| picked.contains(id) || installed.contains(id))
        };
        let mut skipped = Vec::with_capacity(self.skipped.len());
        for mut dep in self.skipped.drain(..) {
            match &mut dep.reason {
                SkipReason::Incompatible { installed } => *installed = present(&dep.project_id),
                SkipReason::AlreadyInstalled => {}
                SkipReason::Optional | SkipReason::Embedded => {
                    if dep
                        .project_id
                        .as_deref()
                        .is_some_and(|id| picked.contains(id))
                    {
                        continue;
                    }
                }
            }
            if !skipped.contains(&dep) {
                skipped.push(dep);
            }
        }
        self.skipped = skipped;

        let mut unresolved = Vec::with_capacity(self.unresolved.len());
        for dep in self.unresolved.drain(..) {
            if !present(&dep.project_id) && !unresolved.contains(&dep) {
                unresolved.push(dep);
            }
        }
        self.unresolved = unresolved;
    }
}

enum Found<'a> {
    Pick(&'a Version, &'a VersionFile),
    Need(Lookup),
    Unresolved(Option<&'a String>, UnresolvedReason),
}

/// Finds the version to install for a required `dep`: the version it pins if
/// that fits, else the one chosen for its project.
fn find<'a>(dep: &'a Dependency, target: Target<'_>, answers: &'a Answers) -> Found<'a> {
    let mut pinned_project = None;
    if let Some(id) = &dep.version_id {
        let lookup = Lookup::Version(id.clone());
        match answers.get(&lookup) {
            None => return Found::Need(lookup),
            Some(Some(version)) => {
                if let Some(jar) = target.jar(version) {
                    return Found::Pick(version, jar);
                }
                pinned_project = Some(&version.project_id);
            }
            Some(None) => {}
        }
    }
    let Some(project) = dep.project_id.as_ref().or(pinned_project) else {
        let reason = if dep.version_id.is_some() {
            UnresolvedReason::NoCompatibleVersion
        } else {
            UnresolvedReason::NotOnModrinth
        };
        return Found::Unresolved(None, reason);
    };
    let lookup = Lookup::Project(project.clone());
    match answers.get(&lookup) {
        None => Found::Need(lookup),
        Some(answer) => match answer.as_ref().and_then(|v| Some((v, target.jar(v)?))) {
            Some((version, jar)) => Found::Pick(version, jar),
            None => Found::Unresolved(Some(project), UnresolvedReason::NoCompatibleVersion),
        },
    }
}

/// A stand-in name until titles are fetched.
fn dep_name(dep: &Dependency) -> String {
    [&dep.project_id, &dep.file_name, &dep.version_id]
        .into_iter()
        .find_map(Clone::clone)
        .unwrap_or_else(|| "unknown".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modrinth::model::tests::version;

    const TARGET: Target<'static> = Target {
        game_version: "1.21.4",
        loader: "fabric",
    };

    fn dep(kind: DependencyType, project: Option<&str>, pin: Option<&str>) -> Dependency {
        Dependency {
            project_id: project.map(str::to_owned),
            version_id: pin.map(str::to_owned),
            file_name: None,
            dependency_type: kind,
        }
    }

    fn req(project: &str) -> Dependency {
        dep(DependencyType::Required, Some(project), None)
    }

    /// Version `<project>-v` of `project`, needing `deps`.
    fn with_deps(project: &str, deps: &[Dependency]) -> Version {
        Version {
            dependencies: deps.to_vec(),
            ..version(project, &format!("{project}-v"), 1)
        }
    }

    /// An answer for a project: its version `<project>-v`, needing `deps`.
    fn has(project: &str, deps: &[Dependency]) -> (Lookup, Option<Version>) {
        (
            Lookup::Project(project.to_owned()),
            Some(with_deps(project, deps)),
        )
    }

    fn pick(version: Version) -> Pick {
        let jar = version.jar().unwrap().clone();
        Pick { version, jar }
    }

    /// An answer for the pinned version `id` of `project`.
    fn pin(project: &str, id: &str) -> (Lookup, Option<Version>) {
        (
            Lookup::Version(id.to_owned()),
            Some(version(project, id, 1)),
        )
    }

    /// A version of project `a` for another Minecraft.
    fn old_game() -> Version {
        Version {
            game_versions: vec!["1.20.1".to_owned()],
            ..version("a", "a-old", 1)
        }
    }

    /// A plan, as picked version ids and (name, reason) pairs.
    type Summary = (
        Vec<String>,
        Vec<(String, SkipReason)>,
        Vec<(String, UnresolvedReason)>,
    );

    /// A named case: the root's dependencies, the answers known, the installed
    /// project ids, and the result wanted.
    type Case = (
        &'static str,
        Vec<Dependency>,
        Vec<(Lookup, Option<Version>)>,
        Vec<&'static str>,
        Result<Summary, Vec<Lookup>>,
    );

    fn check(cases: impl IntoIterator<Item = Case>) {
        for (case, root_deps, answers, installed, want) in cases {
            let root = pick(with_deps("root", &root_deps));
            let answers = answers.into_iter().collect();
            let installed = installed.into_iter().map(str::to_owned).collect();
            let got = resolve(&root, TARGET, &answers, &installed).map(|plan| {
                (
                    plan.picks.into_iter().map(|p| p.version.id).collect(),
                    plan.skipped
                        .into_iter()
                        .map(|d| (d.name, d.reason))
                        .collect(),
                    plan.unresolved
                        .into_iter()
                        .map(|d| (d.name, d.reason))
                        .collect(),
                )
            });
            assert_eq!(got, want, "{case}");
        }
    }

    fn picks(ids: &[&str]) -> Summary {
        plan(ids, &[], &[])
    }

    fn plan(
        ids: &[&str],
        skipped: &[(&str, SkipReason)],
        unresolved: &[(&str, UnresolvedReason)],
    ) -> Summary {
        let own = |s: &str| s.to_owned();
        (
            ids.iter().copied().map(own).collect(),
            skipped.iter().map(|&(n, r)| (own(n), r)).collect(),
            unresolved.iter().map(|&(n, r)| (own(n), r)).collect(),
        )
    }

    #[test]
    fn walks_required_dependencies_breadth_first() {
        check([
            (
                "no dependencies",
                vec![],
                vec![],
                vec![],
                Ok(picks(&["root-v"])),
            ),
            (
                "asks for what it doesn't know, once each",
                vec![req("a"), req("b"), req("a")],
                vec![],
                vec![],
                Err(vec![
                    Lookup::Project("a".to_owned()),
                    Lookup::Project("b".to_owned()),
                ]),
            ),
            (
                "asks for the next level",
                vec![req("a")],
                vec![has("a", &[req("b")])],
                vec![],
                Err(vec![Lookup::Project("b".to_owned())]),
            ),
            (
                "a shared dependency once",
                vec![req("a"), req("b")],
                vec![has("a", &[req("c")]), has("b", &[req("c")]), has("c", &[])],
                vec![],
                Ok(picks(&["root-v", "a-v", "b-v", "c-v"])),
            ),
            (
                "cycles end",
                vec![req("a")],
                vec![has("a", &[req("root"), req("b")]), has("b", &[req("a")])],
                vec![],
                Ok(picks(&["root-v", "a-v", "b-v"])),
            ),
            (
                "installed dependencies are skipped without a lookup",
                vec![req("a")],
                vec![],
                vec!["a"],
                Ok(plan(
                    &["root-v"],
                    &[("a", SkipReason::AlreadyInstalled)],
                    &[],
                )),
            ),
        ]);
    }

    #[test]
    fn skips_what_is_not_required() {
        use DependencyType::{Embedded, Incompatible, Optional};

        check([
            (
                "optional, embedded, and incompatible",
                vec![
                    dep(Optional, Some("o"), None),
                    dep(Embedded, Some("e"), None),
                    dep(Incompatible, Some("i"), None),
                    dep(Incompatible, Some("j"), None),
                ],
                vec![],
                vec!["j"],
                Ok(plan(
                    &["root-v"],
                    &[
                        ("o", SkipReason::Optional),
                        ("e", SkipReason::Embedded),
                        ("i", SkipReason::Incompatible { installed: false }),
                        ("j", SkipReason::Incompatible { installed: true }),
                    ],
                    &[],
                )),
            ),
            (
                "optional here but required elsewhere is installed",
                vec![dep(Optional, Some("a"), None), req("b")],
                vec![has("a", &[]), has("b", &[req("a")])],
                vec![],
                Ok(picks(&["root-v", "b-v", "a-v"])),
            ),
            (
                "incompatible with something being installed conflicts",
                vec![req("a"), dep(Incompatible, Some("a"), None)],
                vec![has("a", &[])],
                vec![],
                Ok(plan(
                    &["root-v", "a-v"],
                    &[("a", SkipReason::Incompatible { installed: true })],
                    &[],
                )),
            ),
        ]);
    }

    #[test]
    fn reports_required_dependencies_it_cannot_install() {
        use UnresolvedReason::{NoCompatibleVersion, NotOnModrinth};

        let external = Dependency {
            file_name: Some("lib.jar".to_owned()),
            ..dep(DependencyType::Required, None, None)
        };
        check([
            (
                "no fitting version, reported once",
                vec![req("a"), req("b"), req("c")],
                vec![
                    (Lookup::Project("a".to_owned()), None),
                    (Lookup::Project("b".to_owned()), Some(old_game())),
                    has("c", &[req("a")]),
                ],
                vec![],
                Ok(plan(
                    &["root-v", "c-v"],
                    &[],
                    &[("a", NoCompatibleVersion), ("b", NoCompatibleVersion)],
                )),
            ),
            (
                "a file from elsewhere",
                vec![external],
                vec![],
                vec![],
                Ok(plan(&["root-v"], &[], &[("lib.jar", NotOnModrinth)])),
            ),
            (
                "a missing pin with no project",
                vec![dep(DependencyType::Required, None, Some("gone"))],
                vec![(Lookup::Version("gone".to_owned()), None)],
                vec![],
                Ok(plan(&["root-v"], &[], &[("gone", NoCompatibleVersion)])),
            ),
        ]);
    }

    #[test]
    fn follows_pinned_versions_that_fit() {
        let pinned = |project, id| dep(DependencyType::Required, project, Some(id));
        check([
            (
                "a fitting pin is used",
                vec![pinned(Some("a"), "a-1")],
                vec![pin("a", "a-1")],
                vec![],
                Ok(picks(&["root-v", "a-1"])),
            ),
            (
                "a pin that doesn't fit falls back to its project",
                vec![pinned(None, "a-old")],
                vec![
                    (Lookup::Version("a-old".to_owned()), Some(old_game())),
                    has("a", &[]),
                ],
                vec![],
                Ok(picks(&["root-v", "a-v"])),
            ),
            (
                "a pin-only dependency on a project already picked",
                vec![req("a"), pinned(None, "a-1")],
                vec![has("a", &[]), pin("a", "a-1")],
                vec![],
                Ok(picks(&["root-v", "a-v"])),
            ),
            (
                "a pin-only dependency on an installed project",
                vec![pinned(None, "a-1")],
                vec![pin("a", "a-1")],
                vec!["a"],
                Ok(plan(
                    &["root-v"],
                    &[("a", SkipReason::AlreadyInstalled)],
                    &[],
                )),
            ),
        ]);
    }
}
