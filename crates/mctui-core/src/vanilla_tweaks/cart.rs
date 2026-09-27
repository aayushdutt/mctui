//! The cart: the packs picked for an instance, and the ones its built pack
//! holds. Saved beside the pack as `.mctui-vanillatweaks.json` in the Go app's
//! format: `{"version": "1.21", "packs": {category: [ids]}, "applied": {…}}`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{Catalog, CatalogVersion, Error};
use crate::{Loaded, fsutil};

/// The cart's file name, in `.minecraft/resourcepacks`.
pub(crate) const CART_FILE: &str = ".mctui-vanillatweaks.json";

/// The packs picked for one instance, and the ones in its built
/// `VanillaTweaks.zip` as of the last apply.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cart {
    /// The catalog version of the last build, e.g. "1.21"; empty when unknown.
    /// (The Go app wrote the version the cart was started with.)
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    packs: Selection,
    #[serde(default, skip_serializing_if = "Selection::is_empty")]
    applied: Selection,
}

/// A pack in the cart: the category it's filed under and its id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackRef {
    /// The name of the category the pack is directly in.
    pub category: String,
    /// The pack's id.
    pub name: String,
}

impl PackRef {
    fn new(category: &str, name: &str) -> Self {
        Self {
            category: category.to_owned(),
            name: name.to_owned(),
        }
    }
}

/// Where a pack stands between the cart and the built pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackState {
    /// Neither picked nor built.
    Unpicked,
    /// Picked and in the built pack.
    Built,
    /// Picked since the last build: the next apply adds it.
    PendingAdd,
    /// Built, but no longer picked: the next apply removes it.
    PendingRemoval,
}

/// What [`Cart::toggle`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Toggled {
    /// Picked the pack, unpicking these incompatible ones.
    Added { replaced: Vec<PackRef> },
    /// Unpicked the pack.
    Removed,
}

impl Cart {
    /// Where an instance's cart lives, given its game dir.
    pub fn path(game_dir: &Path) -> PathBuf {
        game_dir.join("resourcepacks").join(CART_FILE)
    }

    /// Reads an instance's cart. No file is an empty cart. A damaged file is
    /// moved aside, and an unreadable one left alone, each with a notice; the
    /// cart then starts empty.
    pub async fn load(game_dir: &Path) -> Loaded<Cart> {
        let path = Self::path(game_dir);
        tokio::task::spawn_blocking(move || read(&path))
            .await
            .unwrap_or_else(|err| Loaded {
                value: Cart::default(),
                notice: Some(format!("Couldn't read your Vanilla Tweaks picks ({err}).")),
            })
    }

    /// Saves the cart atomically, creating the resourcepacks folder if needed.
    pub async fn save(&self, game_dir: &Path) -> Result<(), Error> {
        let json = serde_json::to_vec_pretty(self);
        super::blocking(Self::path(game_dir), move |path| {
            let json = json?;
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)?;
            }
            fsutil::write_atomic(path, &json, 0o644)
        })
        .await
    }

    /// Whether `name` is picked under `category`.
    pub fn contains(&self, category: &str, name: &str) -> bool {
        self.packs.contains(category, name)
    }

    /// How many packs are picked.
    pub fn len(&self) -> usize {
        self.packs.len()
    }

    /// Whether nothing is picked. The built pack may still hold packs; see
    /// [`Cart::has_unbuilt_changes`].
    pub fn is_empty(&self) -> bool {
        self.packs.is_empty()
    }

    /// How many packs are picked directly in `category`.
    pub fn count_in(&self, category: &str) -> usize {
        self.packs.count_in(category)
    }

    /// Where the pack stands: picked, built, both, or neither.
    pub fn state(&self, category: &str, name: &str) -> PackState {
        let picked = self.packs.contains(category, name);
        let built = self.applied.contains(category, name);
        match (picked, built) {
            (true, true) => PackState::Built,
            (true, false) => PackState::PendingAdd,
            (false, true) => PackState::PendingRemoval,
            (false, false) => PackState::Unpicked,
        }
    }

    /// Every pack that's picked or built, by category then id, with its
    /// state: the cart's rows, pending removals included.
    pub fn entries(&self) -> impl Iterator<Item = (PackRef, PackState)> + '_ {
        let all: BTreeSet<(&str, &str)> = self.packs.iter().chain(self.applied.iter()).collect();
        all.into_iter()
            .map(|(category, name)| (PackRef::new(category, name), self.state(category, name)))
    }

    /// Whether the picks differ from the built pack, so applying would
    /// change it.
    pub fn has_unbuilt_changes(&self) -> bool {
        self.packs != self.applied
    }

    /// The picked packs that picking `name` would replace, being incompatible
    /// with it ("⇄ replaces X"). Empty when `name` is picked already.
    pub fn conflicts(&self, catalog: &Catalog, name: &str) -> Vec<PackRef> {
        if self.packs.iter().any(|(_, picked)| picked == name) {
            return Vec::new();
        }
        let incompatible = catalog.incompatible_with(name);
        self.packs
            .iter()
            .filter(|(_, picked)| incompatible.contains(picked))
            .map(|(category, picked)| PackRef::new(category, picked))
            .collect()
    }

    /// Picks `name` under `category`, first unpicking the packs it's
    /// incompatible with. Returns those.
    pub fn add(&mut self, catalog: &Catalog, category: &str, name: &str) -> Vec<PackRef> {
        let replaced = self.conflicts(catalog, name);
        for pack in &replaced {
            self.packs.remove(&pack.category, &pack.name);
        }
        self.packs.insert(category, name);
        replaced
    }

    /// Unpicks `name` from `category`. Returns whether it was picked.
    pub fn remove(&mut self, category: &str, name: &str) -> bool {
        self.packs.remove(category, name)
    }

    /// Unpicks the pack if it's picked, else picks it as [`Cart::add`] does.
    pub fn toggle(&mut self, catalog: &Catalog, category: &str, name: &str) -> Toggled {
        if self.remove(category, name) {
            Toggled::Removed
        } else {
            Toggled::Added {
                replaced: self.add(catalog, category, name),
            }
        }
    }

    /// Unpicks everything. The built pack keeps its packs until the next
    /// apply removes them.
    pub fn clear(&mut self) {
        self.packs = Selection::default();
    }

    /// Records a build of exactly the picked packs, for `version`.
    pub(super) fn mark_applied(&mut self, version: CatalogVersion) {
        self.applied = self.packs.clone();
        self.version = version.to_string();
    }

    /// Records that there's no built pack.
    pub(super) fn mark_unapplied(&mut self) {
        self.applied = Selection::default();
    }

    /// The picked packs, for the build request.
    pub(super) fn picked(&self) -> &Selection {
        &self.packs
    }
}

/// Reads the cart at `path`, recovering from a bad file as [`Cart::load`]
/// says.
fn read(path: &Path) -> Loaded<Cart> {
    const CONSEQUENCE: &str = "starting with no packs picked";
    let recovered = |notice| Loaded {
        value: Cart::default(),
        notice: Some(notice),
    };
    match crate::read_optional(path) {
        Ok(None) => Loaded::clean(Cart::default()),
        Ok(Some(data)) => match serde_json::from_slice(&data) {
            Ok(cart) => Loaded::clean(cart),
            Err(e) => recovered(crate::quarantine_malformed(path, &e, CONSEQUENCE).0),
        },
        Err(e) => recovered(crate::unreadable_notice(path, &e, CONSEQUENCE)),
    }
}

/// Category name → pack ids: the shape of the cart file and of a build
/// request. Never holds an empty category, so equal picks compare equal.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "BTreeMap<String, BTreeSet<String>>")]
pub(super) struct Selection(BTreeMap<String, BTreeSet<String>>);

impl From<BTreeMap<String, BTreeSet<String>>> for Selection {
    fn from(mut packs: BTreeMap<String, BTreeSet<String>>) -> Self {
        packs.retain(|_, names| !names.is_empty());
        Self(packs)
    }
}

impl Selection {
    fn contains(&self, category: &str, name: &str) -> bool {
        self.0
            .get(category)
            .is_some_and(|names| names.contains(name))
    }

    fn insert(&mut self, category: &str, name: &str) {
        self.0
            .entry(category.to_owned())
            .or_default()
            .insert(name.to_owned());
    }

    fn remove(&mut self, category: &str, name: &str) -> bool {
        let Some(names) = self.0.get_mut(category) else {
            return false;
        };
        let removed = names.remove(name);
        if names.is_empty() {
            self.0.remove(category);
        }
        removed
    }

    /// `(category, id)` pairs, sorted.
    fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().flat_map(|(category, names)| {
            names
                .iter()
                .map(move |name| (category.as_str(), name.as_str()))
        })
    }

    fn len(&self) -> usize {
        self.0.values().map(BTreeSet::len).sum()
    }

    fn count_in(&self, category: &str) -> usize {
        self.0.get(category).map_or(0, BTreeSet::len)
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The build request's `packs` field: `{"Category":["Pack",…],…}`.
    pub(super) fn to_json(&self) -> String {
        let packs = self
            .0
            .iter()
            .map(|(category, names)| {
                let names: serde_json::Value = names.iter().map(String::as_str).collect();
                (category.clone(), names)
            })
            .collect();
        serde_json::Value::Object(packs).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "Aesthetic": A lists B; C is listed by D; E and F list each other; G is
    /// free. "Terrain" (nested under "World"): X lists A.
    fn catalog() -> Catalog {
        let json = br#"{"categories": [
            {"category": "Aesthetic", "packs": [
                {"name": "A", "display": "A", "incompatible": ["B"]},
                {"name": "B", "display": "B", "incompatible": []},
                {"name": "C", "display": "C", "incompatible": []},
                {"name": "D", "display": "D", "incompatible": ["C"]},
                {"name": "E", "display": "E", "incompatible": ["F"]},
                {"name": "F", "display": "F", "incompatible": ["E"]},
                {"name": "G", "display": "G", "incompatible": []}
            ]},
            {"category": "World", "categories": [
                {"category": "Terrain", "packs": [
                    {"name": "X", "display": "X", "incompatible": ["A"]}
                ]}
            ]}
        ]}"#;
        Catalog::from_json(CatalogVersion::for_minecraft("1.21").unwrap(), json).unwrap()
    }

    fn category_of(name: &str) -> &'static str {
        if name == "X" { "Terrain" } else { "Aesthetic" }
    }

    /// A cart with `names` (space-separated) picked.
    fn cart_with(names: &str) -> Cart {
        let mut cart = Cart::default();
        for name in names.split_whitespace() {
            cart.packs.insert(category_of(name), name);
        }
        cart
    }

    /// The picked ids, space-separated.
    fn picked(cart: &Cart) -> String {
        let names: Vec<_> = cart.packs.iter().map(|(_, name)| name).collect();
        names.join(" ")
    }

    fn refs(names: &str) -> Vec<PackRef> {
        names
            .split_whitespace()
            .map(|name| PackRef::new(category_of(name), name))
            .collect()
    }

    #[test]
    fn toggling_resolves_incompatibilities() {
        let catalog = catalog();
        // (picked before, toggled, what it replaced or `None` if unpicked, picked after)
        for (before, toggle, replaced, after) in [
            ("", "G", Some(""), "G"),
            ("B", "A", Some("B"), "A"),
            // D lists C, so picking C replaces D too.
            ("D", "C", Some("D"), "C"),
            ("E", "F", Some("E"), "F"),
            ("G", "A", Some(""), "A G"),
            ("A G", "A", None, "G"),
            // Across categories, both directions at once.
            ("B X G", "A", Some("B X"), "A G"),
        ] {
            let mut cart = cart_with(before);
            let want = replaced.map_or(Toggled::Removed, |names| Toggled::Added {
                replaced: refs(names),
            });
            let got = cart.toggle(&catalog, category_of(toggle), toggle);
            assert_eq!(got, want, "toggle {toggle} with {before:?}");
            assert_eq!(picked(&cart), after, "toggle {toggle} with {before:?}");
        }
    }

    #[test]
    fn replacing_the_last_pack_of_a_category_drops_the_category() {
        let mut cart = cart_with("X");
        cart.add(&catalog(), "Aesthetic", "A");
        assert_eq!(cart.count_in("Terrain"), 0);
        assert_eq!(cart.packs.0.keys().collect::<Vec<_>>(), ["Aesthetic"]);
    }

    #[test]
    fn conflicts_name_what_a_pick_would_replace() {
        let catalog = catalog();
        let cart = cart_with("B X G");
        assert_eq!(cart.conflicts(&catalog, "A"), refs("B X"));
        assert_eq!(cart.conflicts(&catalog, "C"), vec![]);
        assert_eq!(cart.conflicts(&catalog, "B"), vec![], "already picked");
        assert_eq!(cart.conflicts(&catalog, "Unknown"), vec![]);
    }

    #[test]
    fn states_follow_picks_against_the_build() {
        let catalog = catalog();
        let mut cart = cart_with("A G");
        assert_eq!(cart.state("Aesthetic", "A"), PackState::PendingAdd);
        assert_eq!(cart.state("Aesthetic", "C"), PackState::Unpicked);
        assert!(cart.has_unbuilt_changes());

        cart.mark_applied(catalog.version);
        assert_eq!(cart.version, "1.21");
        assert_eq!(cart.state("Aesthetic", "A"), PackState::Built);
        assert!(!cart.has_unbuilt_changes());

        cart.remove("Aesthetic", "A");
        cart.add(&catalog, "Aesthetic", "C");
        let entries: Vec<_> = cart
            .entries()
            .map(|(pack, state)| (pack.name, state))
            .collect();
        assert_eq!(
            entries,
            [
                ("A".to_owned(), PackState::PendingRemoval),
                ("C".to_owned(), PackState::PendingAdd),
                ("G".to_owned(), PackState::Built),
            ]
        );
        assert!(cart.has_unbuilt_changes());

        // Back to what was built: no changes, whatever the order of edits.
        cart.toggle(&catalog, "Aesthetic", "C");
        cart.toggle(&catalog, "Aesthetic", "A");
        assert!(!cart.has_unbuilt_changes());
    }

    #[test]
    fn clearing_keeps_the_build_until_the_next_apply() {
        let mut cart = cart_with("A G");
        cart.mark_applied(catalog().version);
        cart.clear();
        assert!(cart.is_empty());
        assert_eq!(cart.len(), 0);
        assert!(cart.has_unbuilt_changes());
        assert!(
            cart.entries()
                .all(|(_, state)| state == PackState::PendingRemoval)
        );
        cart.mark_unapplied();
        assert!(!cart.has_unbuilt_changes());
        assert_eq!(cart.entries().count(), 0);
    }

    #[test]
    fn counts() {
        let cart = cart_with("A G X");
        assert_eq!(cart.len(), 3);
        assert_eq!(cart.count_in("Aesthetic"), 2);
        assert_eq!(cart.count_in("World"), 0, "only direct picks count");
        assert!(cart.contains("Terrain", "X"));
        assert!(!cart.contains("Aesthetic", "X"));
    }

    #[test]
    fn reads_the_go_format() {
        // As Go wrote it: insertion order, and `applied` absent until built.
        let cart: Cart = serde_json::from_str(
            r#"{"version": "1.21", "packs": {"Aesthetic": ["G", "A"], "Empty": []}}"#,
        )
        .unwrap();
        assert_eq!(cart.version, "1.21");
        assert_eq!(picked(&cart), "A G");
        assert_eq!(cart.count_in("Empty"), 0);
        assert!(cart.has_unbuilt_changes());

        let empty: Cart = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, Cart::default());
    }

    #[test]
    fn writes_the_go_format() {
        let mut cart = cart_with("G A X");
        let json = serde_json::to_string_pretty(&cart).unwrap();
        assert_eq!(
            json,
            r#"{
  "version": "",
  "packs": {
    "Aesthetic": [
      "A",
      "G"
    ],
    "Terrain": [
      "X"
    ]
  }
}"#
        );
        cart.mark_applied(catalog().version);
        cart.remove("Terrain", "X");
        let reread: Cart = serde_json::from_str(&serde_json::to_string(&cart).unwrap()).unwrap();
        assert_eq!(reread, cart);
    }

    #[test]
    fn build_request_packs() {
        let cart = cart_with("G A X");
        assert_eq!(
            cart.picked().to_json(),
            r#"{"Aesthetic":["A","G"],"Terrain":["X"]}"#
        );
        assert_eq!(Cart::default().picked().to_json(), "{}");
    }

    #[tokio::test]
    async fn load_and_save() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();

        let missing = Cart::load(game).await;
        assert_eq!(missing.value, Cart::default());
        assert!(missing.notice.is_none());

        let cart = cart_with("A");
        cart.save(game).await.unwrap();
        let loaded = Cart::load(game).await;
        assert_eq!(loaded.value, cart);
        assert!(loaded.notice.is_none());
    }

    #[tokio::test]
    async fn a_damaged_cart_is_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = Cart::path(dir.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{not json").unwrap();

        let loaded = Cart::load(dir.path()).await;
        assert_eq!(loaded.value, Cart::default());
        let notice = loaded.notice.unwrap();
        assert!(notice.contains("starting with no packs picked"), "{notice}");
        assert!(!path.exists(), "moved aside, so a save can't destroy it");
    }
}
