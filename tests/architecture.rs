//! Architecture checks: the design limits in `docs/design/architecture.md`, read from the source.
//!
//! Each limit records today's truth and may only get stricter. A check fails when the code gets
//! worse than its limit, and also when it gets better, so the limit is lowered in the same PR and
//! the gain can't quietly slip back. Test code is not checked: anything behind `#[cfg(test)]` is
//! skipped, except the count of tests that reach into Think's private conversation state.

use std::{
    fs,
    path::{Path, PathBuf},
};

use syn::{Attribute, Item, UseTree, spanned::Spanned, visit::Visit};

/// Most fields `BrainDump` may have. Since #37 it holds its parts (the conversation, screen,
/// shaping call and handoff) rather than their fields.
const BRAIN_DUMP_FIELDS: usize = 13;
/// Most `pub` fields `BrainDump` may have. The menu reads `exit()` and `saved()` instead.
const BRAIN_DUMP_PUB_FIELDS: usize = 0;
/// Most non-test source lines longer than [`LONG_LINE`] characters. Since #38 the model prompts
/// live in their own files; what remains is help and display text.
const LONG_LINES: usize = 6;
const LONG_LINE: usize = 200;
/// Most production lines a module may have. Modules already past it are capped at their size in
/// [`BIG_MODULES`] and may shrink freely; growing one means splitting it first.
const MODULE_LINES: usize = 1000;
/// Modules over [`MODULE_LINES`] on 2026-11-06 (#747), with their production line counts.
const BIG_MODULES: &[(&str, usize)] = &[
    ("src/shape.rs", 2528),
    ("src/think/brain_dump.rs", 1714),
    ("src/menu.rs", 1313),
    ("src/seed_me/answering.rs", 1210),
    ("src/seed_me/ledger.rs", 1125),
    ("src/watch.rs", 1039),
];
/// Most calls to the test-only accessors on Think's `Conversation`. Tests that poke private state
/// break on refactors that keep behavior; new tests drive Think through its real input instead.
const CONVERSATION_TEST_ACCESS: usize = 77;
const CONVERSATION_TEST_ACCESSORS: &[&str] = &[
    "sources_mut",
    "fragments_mut",
    "settled_mut",
    "set_last_failure",
    "set_changed_lines",
    "set_board",
    "set_applied",
    "skipped_mut",
    "set_scope_pending",
    "ask_counts_mut",
    "ask_counts",
];

/// Every top-level module and its area. Each phase has its own module; the menu (which also lists
/// the unbuilt places) is the only one that knows every phase; `shared` is code any phase may use.
/// A new module must be listed here, so a new phase can't count as shared code by default.
const MODULES: &[(&str, &str)] = &[
    ("actions", "shared"),
    ("approve", "approve"),
    ("goal", "shared"),
    ("measure", "measure"),
    ("menu", "menu"),
    ("model_setup", "shared"),
    ("pi", "shared"),
    ("repo", "shared"),
    ("sdlc", "shared"),
    ("seed_me", "shared"),
    ("shape", "shape"),
    ("tend", "tend"),
    ("think", "think"),
    ("think_record", "shared"),
    ("voice", "shared"),
    ("watch", "watch"),
    ("worker", "shared"),
    ("taste", "taste"),
];

#[test]
fn every_top_level_module_has_an_area() {
    let lib = syn::parse_file(&fs::read_to_string(root().join("src/lib.rs")).unwrap()).unwrap();
    let declared: Vec<_> = lib
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Mod(m) if !is_test(&m.attrs) => Some(m.ident.to_string()),
            _ => None,
        })
        .collect();
    let unlisted: Vec<_> = declared
        .iter()
        .filter(|d| !MODULES.iter().any(|(m, _)| m == d))
        .collect();
    assert!(
        unlisted.is_empty(),
        "src/lib.rs declares modules MODULES doesn't list; add each with its area: {unlisted:?}"
    );
    let missing: Vec<_> = MODULES
        .iter()
        .filter(|(m, _)| !declared.iter().any(|d| d == m))
        .collect();
    assert!(
        missing.is_empty(),
        "MODULES lists modules src/lib.rs no longer declares: {missing:?}"
    );
}

/// The area a module path (without `crate`) belongs to. Items at the crate root are shared.
fn area(path: &[String]) -> &'static str {
    path.first()
        .and_then(|first| MODULES.iter().find(|(m, _)| m == first))
        .map_or("shared", |(_, area)| area)
}

/// May code in area `from` name code in area `to`? Only the menu knows every phase; anything
/// may use shared code; shared code names no phase.
fn allowed(from: &str, to: &str) -> bool {
    from == to || from == "menu" || to == "shared"
}

#[test]
fn phases_never_reach_each_other() {
    let mut violations = vec![];
    for (file, module) in production_files() {
        let from = area(&module);
        for target in references(&file, &module) {
            let to = area(&target);
            if !allowed(from, to) {
                violations.push(format!(
                    "{} ({from}) uses {} ({to})",
                    rel(&file),
                    target.join("::")
                ));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "A phase reaches into another phase. Go through shared code or the menu instead:\n{}",
        violations.join("\n")
    );
}

/// Lints `src/lib.rs` must deny for the whole library. `missing_docs` makes every reachable item
/// carry an interface comment; `unreachable_pub` makes an item that isn't reachable say so with
/// `pub(super)` or `pub(crate)` instead of `pub`. Together: every `pub` item is documented.
const DENIED_LINTS: &[&str] = &["missing_docs", "unreachable_pub"];

#[test]
fn every_public_item_has_an_interface_comment() {
    let lib = syn::parse_file(&fs::read_to_string(root().join("src/lib.rs")).unwrap()).unwrap();
    let denied: Vec<String> = lib
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("deny"))
        .filter_map(|a| a.meta.require_list().ok())
        .flat_map(|l| {
            l.tokens
                .to_string()
                .split(',')
                .map(|lint| lint.trim().to_owned())
                .collect::<Vec<_>>()
        })
        .collect();
    let missing: Vec<_> = DENIED_LINTS
        .iter()
        .filter(|lint| !denied.iter().any(|d| d == *lint))
        .collect();
    assert!(
        missing.is_empty(),
        "src/lib.rs must keep `#![deny({})]`; missing: {missing:?}",
        DENIED_LINTS.join(", ")
    );
}

#[test]
fn every_module_says_what_it_hides() {
    let mut files: Vec<_> = production_files().into_iter().map(|(f, _)| f).collect();
    files.push(root().join("src/main.rs"));
    let silent: Vec<_> = files
        .iter()
        .filter(|file| {
            let parsed = syn::parse_file(&fs::read_to_string(file).unwrap()).unwrap();
            !parsed.attrs.iter().any(|a| a.path().is_ident("doc"))
        })
        .map(|file| rel(file))
        .collect();
    assert!(
        silent.is_empty(),
        "Each module opens with a `//!` comment saying what it owns and what it promises. \
         Missing in: {silent:?}"
    );
}

#[test]
fn brain_dump_does_not_grow() {
    let file = root().join("src/think/brain_dump.rs");
    let parsed = syn::parse_file(&fs::read_to_string(&file).unwrap()).unwrap();
    let fields = parsed
        .items
        .iter()
        .find_map(|item| match item {
            Item::Struct(s) if s.ident == "BrainDump" => Some(&s.fields),
            _ => None,
        })
        .expect("BrainDump is defined in src/think/brain_dump.rs");
    let public = fields
        .iter()
        .filter(|f| matches!(f.vis, syn::Visibility::Public(_)))
        .count();
    at_limit("BrainDump fields", fields.len(), BRAIN_DUMP_FIELDS);
    at_limit("BrainDump pub fields", public, BRAIN_DUMP_PUB_FIELDS);
}

#[test]
fn long_lines_do_not_grow() {
    let mut files: Vec<_> = production_files().into_iter().map(|(f, _)| f).collect();
    files.push(root().join("src/main.rs"));
    let mut found = vec![];
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        let tests = test_line_ranges(&syn::parse_file(&text).unwrap());
        for (i, line) in text.lines().enumerate() {
            let n = i + 1;
            if line.chars().count() > LONG_LINE && !tests.iter().any(|r| r.contains(&n)) {
                found.push(format!("{}:{n}", rel(&file)));
            }
        }
    }
    at_limit(
        &format!(
            "source lines over {LONG_LINE} characters ({})",
            found.join(", ")
        ),
        found.len(),
        LONG_LINES,
    );
}

#[test]
fn big_modules_do_not_grow() {
    let mut files: Vec<_> = production_files().into_iter().map(|(f, _)| f).collect();
    files.push(root().join("src/main.rs"));
    let over: Vec<_> = files
        .iter()
        .filter_map(|file| {
            let text = fs::read_to_string(file).unwrap();
            let tests = test_line_ranges(&syn::parse_file(&text).unwrap());
            let lines = (1..=text.lines().count())
                .filter(|n| !tests.iter().any(|r| r.contains(n)))
                .count();
            let name = rel(file);
            let limit = BIG_MODULES
                .iter()
                .find(|(big, _)| *big == name)
                .map_or(MODULE_LINES, |(_, cap)| *cap);
            (lines > limit).then(|| format!("{name}: {lines} lines, limit {limit}"))
        })
        .collect();
    assert!(
        over.is_empty(),
        "Modules over their production line limit; split them before adding more: {over:?}"
    );
}

#[test]
fn tests_reach_into_conversation_state_less_over_time() {
    let mut found = 0;
    let mut stack = vec![root().join("src")];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = fs::read_to_string(&path).unwrap();
                found += CONVERSATION_TEST_ACCESSORS
                    .iter()
                    .map(|name| text.matches(&format!(".{name}(")).count())
                    .sum::<usize>();
            }
        }
    }
    at_limit(
        "calls to Conversation's test-only accessors",
        found,
        CONVERSATION_TEST_ACCESS,
    );
}

fn at_limit(what: &str, actual: usize, limit: usize) {
    assert!(
        actual <= limit,
        "{what}: {actual}, over the limit of {limit}. Limits only get stricter."
    );
    assert!(
        actual == limit,
        "{what}: {actual}, better than the limit of {limit}. Lower the limit to {actual} in tests/architecture.rs."
    );
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rel(file: &Path) -> String {
    file.strip_prefix(root())
        .unwrap_or(file)
        .display()
        .to_string()
}

fn is_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && a.meta.require_list().is_ok_and(|l| {
                let cfg = l.tokens.to_string();
                cfg == "test"
                    || (cfg.starts_with("all")
                        && cfg
                            .split(|c: char| !c.is_alphanumeric())
                            .any(|w| w == "test"))
            })
    })
}

/// Every library source file outside `#[cfg(test)]`, with its module path, found by following
/// `mod` declarations from `src/lib.rs`.
fn production_files() -> Vec<(PathBuf, Vec<String>)> {
    let mut out = vec![];
    walk(
        &root().join("src/lib.rs"),
        vec![],
        &root().join("src"),
        &mut out,
    );
    out
}

fn walk(file: &Path, module: Vec<String>, children: &Path, out: &mut Vec<(PathBuf, Vec<String>)>) {
    let parsed = syn::parse_file(&fs::read_to_string(file).unwrap()).unwrap();
    out.push((file.to_path_buf(), module.clone()));
    walk_items(&parsed.items, &module, children, out);
}

fn walk_items(
    items: &[Item],
    module: &[String],
    children: &Path,
    out: &mut Vec<(PathBuf, Vec<String>)>,
) {
    for item in items {
        let Item::Mod(m) = item else { continue };
        if is_test(&m.attrs) {
            continue;
        }
        let name = m.ident.to_string();
        let mut path = module.to_vec();
        path.push(name.clone());
        match &m.content {
            Some(_) => panic!(
                "inline module {} isn't checked; give it its own file",
                path.join("::")
            ),
            None => {
                let flat = children.join(format!("{name}.rs"));
                if flat.is_file() {
                    walk(&flat, path, &children.join(&name), out);
                } else {
                    let dir = children.join(&name);
                    walk(&dir.join("mod.rs"), path, &dir, out);
                }
            }
        }
    }
}

/// Crate paths (without `crate`) named by `use` items and `crate::`/`super::` paths in non-test
/// code of one file.
fn references(file: &Path, module: &[String]) -> Vec<Vec<String>> {
    struct Refs<'a> {
        module: &'a [String],
        found: Vec<Vec<String>>,
    }
    impl Refs<'_> {
        fn resolve(&self, segments: &[String]) -> Option<Vec<String>> {
            let mut base = self.module.to_vec();
            let mut rest = segments;
            match rest.first().map(String::as_str) {
                Some("crate") | Some("tinkery") => {
                    base.clear();
                    rest = &rest[1..];
                }
                Some("super") => {
                    while rest.first().is_some_and(|s| s == "super") {
                        base.pop();
                        rest = &rest[1..];
                    }
                }
                Some("self") => rest = &rest[1..],
                _ => return None,
            }
            base.extend(rest.iter().cloned());
            Some(base)
        }
        fn use_tree(&mut self, prefix: &mut Vec<String>, tree: &UseTree) {
            match tree {
                UseTree::Path(p) => {
                    prefix.push(p.ident.to_string());
                    self.use_tree(prefix, &p.tree);
                    prefix.pop();
                }
                UseTree::Name(n) => self.leaf(prefix, &n.ident.to_string()),
                UseTree::Rename(r) => self.leaf(prefix, &r.ident.to_string()),
                UseTree::Glob(_) => self.leaf(prefix, "self"),
                UseTree::Group(g) => {
                    for t in &g.items {
                        self.use_tree(prefix, t);
                    }
                }
            }
        }
        fn leaf(&mut self, prefix: &[String], name: &str) {
            let mut full = prefix.to_vec();
            if name != "self" {
                full.push(name.into());
            }
            if let Some(path) = self.resolve(&full) {
                self.found.push(path);
            }
        }
    }
    impl<'ast> Visit<'ast> for Refs<'_> {
        fn visit_item(&mut self, item: &'ast Item) {
            let attrs = match item {
                Item::Fn(i) => &i.attrs,
                Item::Impl(i) => &i.attrs,
                Item::Mod(i) => &i.attrs,
                Item::Use(i) => &i.attrs,
                Item::Struct(i) => &i.attrs,
                Item::Enum(i) => &i.attrs,
                Item::Const(i) => &i.attrs,
                Item::Static(i) => &i.attrs,
                Item::Trait(i) => &i.attrs,
                _ => return syn::visit::visit_item(self, item),
            };
            if is_test(attrs) {
                return;
            }
            if let Item::Use(u) = item {
                self.use_tree(&mut vec![], &u.tree);
                return;
            }
            syn::visit::visit_item(self, item);
        }
        fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
            if !is_test(&f.attrs) {
                syn::visit::visit_impl_item_fn(self, f);
            }
        }
        // `pub(super)` names a module but depends on nothing.
        fn visit_visibility(&mut self, _: &'ast syn::Visibility) {}
        fn visit_path(&mut self, path: &'ast syn::Path) {
            let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
            if let Some(resolved) = self.resolve(&segments) {
                self.found.push(resolved);
            }
            syn::visit::visit_path(self, path);
        }
    }
    let parsed = syn::parse_file(&fs::read_to_string(file).unwrap()).unwrap();
    let mut refs = Refs {
        module,
        found: vec![],
    };
    for item in &parsed.items {
        refs.visit_item(item);
    }
    refs.found
}

/// Line ranges of `#[cfg(test)]` items, at any depth of inline modules and impls.
fn test_line_ranges(file: &syn::File) -> Vec<std::ops::RangeInclusive<usize>> {
    struct Ranges(Vec<std::ops::RangeInclusive<usize>>);
    impl Ranges {
        fn add(&mut self, node: &impl Spanned) {
            let span = node.span();
            self.0.push(span.start().line..=span.end().line);
        }
    }
    impl<'ast> Visit<'ast> for Ranges {
        fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
            if is_test(&m.attrs) {
                self.add(m)
            } else {
                syn::visit::visit_item_mod(self, m)
            }
        }
        fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
            if is_test(&f.attrs) {
                self.add(f)
            } else {
                syn::visit::visit_item_fn(self, f)
            }
        }
        fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
            if is_test(&i.attrs) {
                self.add(i)
            } else {
                syn::visit::visit_item_impl(self, i)
            }
        }
        fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
            if is_test(&f.attrs) {
                self.add(f)
            } else {
                syn::visit::visit_impl_item_fn(self, f)
            }
        }
    }
    let mut ranges = Ranges(vec![]);
    ranges.visit_file(file);
    ranges.0
}
