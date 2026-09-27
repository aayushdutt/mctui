//! The loaded screen: the catalog, your cart, and the lists that show them.
//!
//! Everything shown is rebuilt from the catalog and the cart by
//! [`Browser::refresh`] after each change; the lists keep their selections by
//! key through it.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

use mctui_core::vanilla_tweaks::{Cart, Catalog, Category, Pack, PackState, Toggled};
use tokio::sync::watch;

use super::{Msg, Pane};
use crate::fuzzy;
use crate::task::{Task, Tasks};
use crate::ui::input::TextInput;
use crate::ui::list::List;

/// Joins category names into a tree row's key, which doubles as its crumb.
const SEP: &str = " › ";

/// The lists' ids, for their click targets.
pub const CATEGORIES: &str = "packs.categories";
pub const PACKS: &str = "packs.packs";
pub const RESULTS: &str = "packs.results";
pub const CART: &str = "packs.cart";

/// A visible row of the category tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// "GUI › Hearts": the row's key and the packs pane's title.
    pub key: String,
    /// Indices from the top level down.
    pub path: Vec<usize>,
    pub depth: u16,
    pub expandable: bool,
    pub expanded: bool,
    /// Packs picked here and in its subcategories.
    pub picked: usize,
}

impl Node {
    pub fn name(&self) -> &str {
        self.key.rsplit(SEP).next().unwrap_or(&self.key)
    }
}

/// A pack as a list row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackRow {
    pub id: String,
    pub label: String,
    /// The category it's directly in.
    pub category: String,
    pub state: PackState,
    /// Labels of the picked packs that picking this one would replace.
    pub replaces: Vec<String>,
    /// Characters the search matched, for highlighting.
    pub positions: Vec<u32>,
}

/// A row of your pack: a category heading or a pack under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CartRow {
    Group(String),
    Pack(PackRow),
}

impl CartRow {
    fn key(&self) -> String {
        match self {
            // Pack ids are single words, so this can't collide with one.
            CartRow::Group(name) => format!("{GROUP}{name}"),
            CartRow::Pack(row) => row.id.clone(),
        }
    }
}

/// Marks a heading's key in the cart list.
const GROUP: &str = "group: ";

/// Searching every pack.
#[derive(Debug, Clone, Default)]
pub struct Search {
    pub input: TextInput,
    /// Keys go to the field.
    pub typing: bool,
}

/// What a running job is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Work {
    Building,
    Downloading,
    Removing,
}

/// Background work on the instance's pack; dropping it stops the work.
#[derive(Debug)]
pub struct Job {
    /// Its number: results from a job stopped or replaced since are ignored.
    pub seq: u64,
    pub work: Work,
    pub since: Instant,
    _task: Task,
}

impl Job {
    pub fn new(seq: u64, work: Work, since: Instant, task: Task) -> Self {
        Self {
            seq,
            work,
            since,
            _task: task,
        }
    }
}

/// Saves the cart in the background, one save at a time, the newest last.
/// Closing the screen doesn't lose the last pick: the saver stops once it
/// has saved everything it was sent.
#[derive(Debug)]
pub struct Saver {
    latest: watch::Sender<Cart>,
}

impl Saver {
    pub fn start(tasks: &Tasks, game_dir: PathBuf, cart: Cart) -> Self {
        let (latest, mut rx) = watch::channel(cart);
        tasks.spawn_detached(move |mail| async move {
            // Only the newest cart matters: edits made during a save are
            // saved together after it. A cart sent just before the screen
            // closed still counts as changed.
            while rx.changed().await.is_ok() {
                let cart = Cart::clone(&rx.borrow_and_update());
                if let Err(e) = cart.save(&game_dir).await {
                    mail.send(super::app_msg(Msg::SaveFailed(e.to_string())));
                }
            }
        });
        Self { latest }
    }

    fn save(&self, cart: &Cart) {
        self.latest.send_replace(cart.clone());
    }
}

#[derive(Debug)]
pub struct Browser {
    pub catalog: Catalog,
    /// Pack id → its category's path and its index there.
    index: HashMap<String, (Vec<usize>, usize)>,
    pub cart: Cart,
    /// Whether the built pack is switched on in the game.
    pub enabled: bool,
    /// The catalog is the copy from the last visit.
    pub offline: bool,
    expanded: HashSet<String>,
    pub nodes: Vec<Node>,
    pub pack_rows: Vec<PackRow>,
    pub result_rows: Vec<PackRow>,
    pub cart_rows: Vec<CartRow>,
    pub focus: Pane,
    pub categories: List,
    pub packs: List,
    pub results: List,
    pub basket: List,
    pub search: Option<Search>,
    pub job: Option<Job>,
    /// The last job's number.
    pub jobs: u64,
    /// Fetching the catalog again after an offline start.
    pub retry: Option<(Instant, Task)>,
    saver: Saver,
}

impl Browser {
    pub fn new(catalog: Catalog, offline: bool, cart: Cart, enabled: bool, saver: Saver) -> Self {
        // Open the categories whose subcategories hold picks, so they show.
        let mut expanded = HashSet::new();
        expand_picked(&catalog.categories, None, &cart, &mut expanded);
        let mut browser = Self {
            index: index(&catalog),
            catalog,
            cart,
            enabled,
            offline,
            expanded,
            nodes: Vec::new(),
            pack_rows: Vec::new(),
            result_rows: Vec::new(),
            cart_rows: Vec::new(),
            focus: Pane::Categories,
            categories: List::new(CATEGORIES),
            packs: List::new(PACKS),
            results: List::new(RESULTS),
            basket: List::new(CART),
            search: None,
            job: None,
            jobs: 0,
            retry: None,
            saver,
        };
        browser.refresh();
        browser
    }

    /// Swaps in a fresh copy of the catalog.
    pub fn set_catalog(&mut self, catalog: Catalog, offline: bool) {
        self.index = index(&catalog);
        self.catalog = catalog;
        self.offline = offline;
        self.refresh();
    }

    /// Rebuilds every row from the catalog and the cart.
    pub fn refresh(&mut self) {
        let mut nodes = Vec::new();
        tree(
            &self.catalog.categories,
            None,
            &mut Vec::new(),
            (&self.expanded, &self.cart),
            &mut nodes,
        );
        self.categories
            .set_keys(nodes.iter().map(|n| n.key.as_str()));
        self.nodes = nodes;

        let rows = self.selected_category().map_or_else(Vec::new, |category| {
            category
                .packs
                .iter()
                .map(|pack| self.row(category, pack, Vec::new()))
                .collect()
        });
        self.packs.set_keys(keys(&rows));
        self.pack_rows = rows;

        let rows = match &self.search {
            Some(search) => {
                let packs: Vec<_> = self.catalog.packs().collect();
                fuzzy::rank(
                    search.input.as_str(),
                    packs.iter().map(|(_, pack)| pack.label()),
                )
                .into_iter()
                .map(|m| {
                    let (category, pack) = packs[m.index];
                    self.row(category, pack, m.positions)
                })
                .collect()
            }
            None => Vec::new(),
        };
        self.results.set_keys(keys(&rows));
        self.result_rows = rows;

        self.cart_rows = self.cart_rows();
        self.basket
            .set_keys(self.cart_rows.iter().map(CartRow::key));
        self.skip_group(true);
    }

    fn row(&self, category: &Category, pack: &Pack, positions: Vec<u32>) -> PackRow {
        let state = self.cart.state(&category.name, &pack.name);
        let replaces = if state == PackState::Unpicked {
            self.cart
                .conflicts(&self.catalog, &pack.name)
                .iter()
                .map(|p| self.label(&p.name).to_owned())
                .collect()
        } else {
            Vec::new()
        };
        PackRow {
            id: pack.name.clone(),
            label: pack.label().to_owned(),
            category: category.name.clone(),
            state,
            replaces,
            positions,
        }
    }

    /// Your pack's rows: packs picked or built, under their categories, in
    /// the catalog's order. Packs the catalog no longer has go last.
    fn cart_rows(&self) -> Vec<CartRow> {
        let order: HashMap<&str, usize> = self
            .catalog
            .all_categories()
            .enumerate()
            .map(|(i, c)| (c.name.as_str(), i))
            .collect();
        let mut entries: Vec<_> = self.cart.entries().collect();
        entries.sort_by_key(|(pack, _)| {
            let category = order.get(pack.category.as_str()).copied();
            let place = self.index.get(&pack.name).map(|(_, i)| *i);
            (category.is_none(), category, place.is_none(), place)
        });
        let mut rows = Vec::new();
        for (pack, state) in entries {
            if !matches!(rows.last(), Some(CartRow::Pack(last)) if last.category == pack.category) {
                rows.push(CartRow::Group(pack.category.clone()));
            }
            rows.push(CartRow::Pack(PackRow {
                label: self.label(&pack.name).to_owned(),
                id: pack.name,
                category: pack.category,
                state,
                replaces: Vec::new(),
                positions: Vec::new(),
            }));
        }
        rows
    }

    /// The pack with this id, and the category it's directly in.
    pub fn pack(&self, id: &str) -> Option<(&Category, &Pack)> {
        let (path, i) = self.index.get(id)?;
        let category = category_at(&self.catalog, path)?;
        Some((category, category.packs.get(*i)?))
    }

    /// A pack's label, or its id when the catalog doesn't know it.
    pub fn label<'a>(&'a self, id: &'a str) -> &'a str {
        self.pack(id).map_or(id, |(_, pack)| pack.label())
    }

    pub fn selected_node(&self) -> Option<&Node> {
        self.nodes.get(self.categories.selected())
    }

    pub fn selected_category(&self) -> Option<&Category> {
        category_at(&self.catalog, &self.selected_node()?.path)
    }

    /// The row selected in the packs pane: a search result while searching.
    pub fn selected_pack_row(&self) -> Option<&PackRow> {
        if self.search.is_some() {
            self.result_rows.get(self.results.selected())
        } else {
            self.pack_rows.get(self.packs.selected())
        }
    }

    /// The pack selected in the focused pane, if it's packs or your pack.
    pub fn focused_pack(&self) -> Option<&PackRow> {
        match self.focus {
            Pane::Categories => None,
            Pane::Packs => self.selected_pack_row(),
            Pane::Cart => match self.cart_rows.get(self.basket.selected())? {
                CartRow::Pack(row) => Some(row),
                CartRow::Group(_) => None,
            },
        }
    }

    /// Whether the instance has a built pack (picked or since unpicked).
    pub fn is_built(&self) -> bool {
        self.cart
            .entries()
            .any(|(_, state)| matches!(state, PackState::Built | PackState::PendingRemoval))
    }

    /// Whether `b` has anything to do: changes to build, or a built pack
    /// the game has turned off.
    pub fn needs_build(&self) -> bool {
        self.cart.has_unbuilt_changes() || (self.is_built() && !self.enabled)
    }

    /// Picks or unpicks a pack, saving the cart.
    pub fn toggle(&mut self, category: &str, id: &str) -> Toggled {
        let toggled = self.cart.toggle(&self.catalog, category, id);
        self.saver.save(&self.cart);
        self.refresh();
        toggled
    }

    /// Unpicks everything, saving the cart.
    pub fn clear(&mut self) {
        self.cart.clear();
        self.saver.save(&self.cart);
        self.refresh();
    }

    /// The cart after an apply or unapply, which saved it.
    pub fn set_cart(&mut self, cart: Cart, enabled: bool) {
        self.cart = cart;
        self.enabled = enabled;
        self.refresh();
    }

    /// Moves focus to `pane`. Leaving the packs for the categories ends a
    /// search; any other move stops typing into it.
    pub fn focus(&mut self, pane: Pane) -> bool {
        match (pane, &mut self.search) {
            (Pane::Categories, Some(_)) => {
                self.end_search();
            }
            (_, Some(search)) => search.typing = false,
            (_, None) => {}
        }
        if pane == Pane::Cart {
            self.skip_group(true);
        }
        std::mem::replace(&mut self.focus, pane) != pane
    }

    /// The focused list.
    pub fn focused_list(&mut self) -> &mut List {
        match self.focus {
            Pane::Categories => &mut self.categories,
            Pane::Packs if self.search.is_some() => &mut self.results,
            Pane::Packs => &mut self.packs,
            Pane::Cart => &mut self.basket,
        }
    }

    /// Moves the focused list's selection by `delta` rows, refreshing what
    /// depends on it. Returns whether it moved.
    pub fn step(&mut self, delta: isize) -> bool {
        let moved = self.focused_list().step(delta);
        self.after_move(delta >= 0);
        moved
    }

    /// After the focused list's selection moved (`down` or up): the packs
    /// follow the category, and your pack's headings are stepped over.
    pub fn after_move(&mut self, down: bool) {
        match self.focus {
            Pane::Categories => self.refresh(),
            Pane::Cart => self.skip_group(down),
            Pane::Packs => {}
        }
    }

    /// Keeps your pack's selection off its headings, moving `down` if it can.
    fn skip_group(&mut self, down: bool) {
        let on_group = |b: &Self| {
            matches!(
                b.cart_rows.get(b.basket.selected()),
                Some(CartRow::Group(_))
            )
        };
        if on_group(self) {
            let delta = if down || self.basket.selected() == 0 {
                1
            } else {
                -1
            };
            self.basket.step(delta);
        }
    }

    /// Selects the category row `key`.
    pub fn select_category(&mut self, key: &str) -> bool {
        let moved = self.categories.select_key(key);
        if moved {
            self.refresh();
        }
        moved
    }

    /// Opens or closes a category's subcategories.
    pub fn set_expanded(&mut self, key: &str, open: bool) -> bool {
        let changed = if open {
            self.expanded.insert(key.to_owned())
        } else {
            self.expanded.remove(key)
        };
        if changed {
            self.refresh();
        }
        changed
    }

    /// → in the tree: opens a closed category, steps into an open one that
    /// has no packs of its own, else moves on to the packs.
    pub fn right(&mut self) -> bool {
        if self.focus != Pane::Categories {
            return match self.focus {
                Pane::Packs => self.focus(Pane::Cart),
                _ => false,
            };
        }
        let Some(node) = self.selected_node().cloned() else {
            return false;
        };
        if node.expandable && !node.expanded {
            return self.set_expanded(&node.key, true);
        }
        let own_packs = self
            .selected_category()
            .is_some_and(|c| !c.packs.is_empty());
        if node.expandable && !own_packs {
            return self.step(1);
        }
        self.focus(Pane::Packs)
    }

    /// ← in the tree: closes an open category, else goes to its parent.
    pub fn left(&mut self) -> bool {
        match self.focus {
            Pane::Categories => {}
            Pane::Packs => return self.focus(Pane::Categories),
            Pane::Cart => return self.focus(Pane::Packs),
        }
        let Some(node) = self.selected_node().cloned() else {
            return false;
        };
        if node.expanded {
            return self.set_expanded(&node.key, false);
        }
        match node.key.rsplit_once(SEP) {
            Some((parent, _)) => self.select_category(parent),
            None => false,
        }
    }

    /// Shows pack `id` where it lives: its category selected (with its
    /// parents open), and the pack selected among its packs.
    pub fn reveal(&mut self, id: &str) -> bool {
        let Some((path, _)) = self.index.get(id).cloned() else {
            return false;
        };
        let mut keys: Vec<String> = Vec::new();
        let mut level = &self.catalog.categories;
        for i in path {
            let Some(category) = level.get(i) else {
                return false;
            };
            keys.push(match keys.last() {
                Some(parent) => format!("{parent}{SEP}{}", category.name),
                None => category.name.clone(),
            });
            level = &category.subcategories;
        }
        let Some((key, parents)) = keys.split_last() else {
            return false;
        };
        self.expanded.extend(parents.iter().cloned());
        self.search = None;
        self.refresh();
        self.select_category(key);
        self.packs.select_key(id);
        self.focus = Pane::Packs;
        true
    }

    /// Starts typing a search (again).
    pub fn start_search(&mut self) {
        let search = self.search.take().unwrap_or_default();
        self.search = Some(Search {
            typing: true,
            ..search
        });
        self.focus = Pane::Packs;
        self.refresh();
        self.results.select(0);
    }

    /// Stops typing; the results stay.
    pub fn stop_typing(&mut self) -> bool {
        match &mut self.search {
            Some(search) if search.typing => {
                search.typing = false;
                true
            }
            _ => false,
        }
    }

    /// Back to the categories' packs.
    pub fn end_search(&mut self) -> bool {
        let ended = self.search.take().is_some();
        if ended {
            self.refresh();
        }
        ended
    }

    /// The search field changed.
    pub fn searched(&mut self) {
        self.refresh();
        // A new query ranks afresh: the best match first.
        self.results.select(0);
    }
}

fn keys(rows: &[PackRow]) -> impl Iterator<Item = &str> {
    rows.iter().map(|row| row.id.as_str())
}

/// The category at `path` (top-level index first).
fn category_at<'a>(catalog: &'a Catalog, path: &[usize]) -> Option<&'a Category> {
    let (first, rest) = path.split_first()?;
    rest.iter()
        .try_fold(catalog.categories.get(*first)?, |category, &i| {
            category.subcategories.get(i)
        })
}

/// Where every pack is, by id: its category's path and its index there.
fn index(catalog: &Catalog) -> HashMap<String, (Vec<usize>, usize)> {
    fn walk(
        categories: &[Category],
        path: &mut Vec<usize>,
        out: &mut HashMap<String, (Vec<usize>, usize)>,
    ) {
        for (i, category) in categories.iter().enumerate() {
            path.push(i);
            for (j, pack) in category.packs.iter().enumerate() {
                out.entry(pack.name.clone())
                    .or_insert_with(|| (path.clone(), j));
            }
            walk(&category.subcategories, path, out);
            path.pop();
        }
    }
    let mut out = HashMap::new();
    walk(&catalog.categories, &mut Vec::new(), &mut out);
    out
}

/// The visible tree rows: every category, and the subcategories of the
/// expanded ones.
fn tree(
    categories: &[Category],
    parent: Option<&str>,
    path: &mut Vec<usize>,
    (expanded, cart): (&HashSet<String>, &Cart),
    out: &mut Vec<Node>,
) {
    for (i, category) in categories.iter().enumerate() {
        path.push(i);
        let key = match parent {
            Some(parent) => format!("{parent}{SEP}{}", category.name),
            None => category.name.clone(),
        };
        let expandable = !category.subcategories.is_empty();
        let open = expandable && expanded.contains(&key);
        out.push(Node {
            key: key.clone(),
            path: path.clone(),
            depth: path.len() as u16 - 1,
            expandable,
            expanded: open,
            picked: picked(category, cart),
        });
        if open {
            tree(
                &category.subcategories,
                Some(&key),
                path,
                (expanded, cart),
                out,
            );
        }
        path.pop();
    }
}

/// Packs picked in `category` and its subcategories.
fn picked(category: &Category, cart: &Cart) -> usize {
    cart.count_in(&category.name)
        + category
            .subcategories
            .iter()
            .map(|sub| picked(sub, cart))
            .sum::<usize>()
}

/// Adds the keys of the categories whose subcategories hold picks.
fn expand_picked(
    categories: &[Category],
    parent: Option<&str>,
    cart: &Cart,
    out: &mut HashSet<String>,
) {
    for category in categories {
        let key = match parent {
            Some(parent) => format!("{parent}{SEP}{}", category.name),
            None => category.name.clone(),
        };
        if category
            .subcategories
            .iter()
            .any(|sub| picked(sub, cart) > 0)
        {
            out.insert(key.clone());
        }
        expand_picked(&category.subcategories, Some(&key), cart, out);
    }
}
